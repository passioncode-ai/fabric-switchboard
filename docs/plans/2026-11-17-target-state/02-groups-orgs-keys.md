# 02 Groups, organizations, seats and keys: target state against the code (2026-11-17)

Part of the [target-state plan](README.md). Read at `f5e2c77e` (origin/main, branch
`target-state`). Nothing here was run: `cargo` and `npm` were not invoked (the disk had 2.7 GB
free), so every "Now" is read from the source and its tests, and a test named as proof was read,
not run.

**What the area is for.** Who pays for which work. A group (tool names: project, pool) holds
repositories and the accounts that serve sessions in them; an organization labels whose work an
account does; a seat is an account working for an organization, here, on another machine of the
person, or on a teammate's machine under a lease; keys and ceilings decide what paid agents may
spend. It serves North Star §3 (a person's Claude seat), §4 (accounts and seats per group and
organization, fallback chains, ceilings), §6 (work continues on the next account), §10 (seats at
boot) and gaps G1 (SB-295), G2 (SB-291), G4 (SB-292), G14 (SB-301) as listed in
[AGENTS.md](../../../AGENTS.md) "Read first" 1a.

Abbreviations: core = `crates/switchboard-core/src`, rt = `crates/switchboard-runtime/src`,
cli = `crates/switchboard-cli/src`, proxy = `crates/switchboard-proxy/src`.

---

### 1. Groups and their folders

**Target.** A group is a name, one or more folders (repositories) and the accounts it holds or
shares; a repository may be in several groups as itself, never nested inside another group's
folder; a repository in no group stays in `default` and is never made a group of its own
([DEC-0007](../../DECISIONS.md#dec-0007-a-repository-in-no-group-stays-in-the-default-group),
CONTRACTS "Projects (0.6)"). Every window action on a group has an MCP tool (DEC-0019). A refusal
says in one sentence what to change (CONTRACTS "One sentence per refusal").

**Now.** `Project` at core/projects.rs:210; save by delta at core/projects.rs:955
(`save_project_with`), off switch at core/projects.rs:848, delete at core/projects.rs:1595,
sources at core/projects.rs:1560. Publication checks in `validate_projects`
(core/projects.rs:568): at most 64 groups, 16 sources, 1024 folders in all
(core/projects.rs:447-451), no nesting across groups (core/projects.rs:647-654, O(n^2) over all
folders). MCP `switchboard_project_create|edit|delete|move_root`; parity test
`every_window_command_has_an_mcp_tool_or_a_reason` (src-tauri/src/parity.rs:223). Tests:
core/tests/projects.rs (20 cases, e.g. `a_source_may_be_a_group_path`,
`an_account_whose_own_group_goes_away_is_no_longer_shared_and_never_moves_by_itself`),
cli/tests/mcp.rs `a_project_is_listed_and_changed_by_delta_over_mcp`.

**Gap.** P2: every structural refusal in `validate_projects` is the same string "Invalid project
metadata" (core/projects.rs:625, 632, 645, 651), so a nested folder or a bad source reached by a
path other than `save_project_with` cannot be told apart by the person or an agent. P3: the
1024-folder cap counts worktrees too (SB-171 found 126 folders in one group before cleanup); no
test names the cap's refusal sentence. P3: SB-07 (flaky `project_rules_are_optional_visible_and_applied_on_request`) and SB-362 (owner test stack depth on `a_project_changes_...`) still open.

**Refactor.** Typed `ProjectError` enum in core with one sentence each, mapped by the operation
layer; replace the pairwise nesting scan with a sorted-prefix pass (folders sorted, compare
neighbours).

### 2. Folder discovery: worktrees, clones, sources, deletion

**Target.** A worktree or clone of a group's repository, or a repository of one of its sources
(`github.com/<owner>`), joins that group by itself with one notification; a deleted folder leaves
by itself; nothing joins the pay-per-token group by itself (DEC-0023 D-3); a folder never joins
because an agent merely worked there (SB-171, operator 2026-10-22). Reads Git metadata only and
never under the owner transaction (AGENTS.md Lifecycle, background monitor).

**Now.** rt/suggest.rs (1933 lines): `project-suggestions.json` rows `added|pending|dismissed|gone`
(rt/suggest.rs:1-12); `worked` rows no longer join
(`a_folder_never_joins_a_group_because_its_agent_worked_there`, rt/suggest.rs:1345); worktree and
clone recognition `worktrees_clones_and_ecosystem_repositories_are_recognised` (1420); temp and
cache folders refused (1503); scan reads only changed parents (1831). Project roots and the disk
map in rt/roots.rs (10 tests, e.g. `worktrees_and_clones_go_under_their_main_checkout`, 707).
Folder attribution vectors in rt/vectors (SB-141, SB-391).

**Gap.** P2: the decisions contradict the code and each other. DEC-0007 still says "a repository a
group's agents create or work in joins that group by itself", the MCP server's instruction text
repeats it, and rt/suggest.rs:10-12 documents SB-131 `worked` joins; the code (SB-171) refuses
them. P2: SB-141 (one attribution rule set across Observatory, Switchboard and Fabric) and SB-391
(DEC-0177 re-pin, in review) open. P3: SB-234 sibling flakiness under load.

**Refactor.** Amend DEC-0007 (a new DEC superseding its "work in" half) and the module doc; delete
the dead `Reason::Worked` join path after one release that still reads old rows.

### 3. Which group serves a folder

**Target.** A session's group is decided by its folder alone and is predictable: of the groups
holding the folder, an on group before an off one, the more private context first when
organizations exist, then the first with an enabled account (DEC-0023 D-15, SCN-070 step 5). The
person can see which group a folder resolves to (`switchboard_project_context`).

**Now.** `Snapshot::project_for` (core/projects.rs:668-682): personal groups first when
organizations exist, off groups last and dropped while an on group holds the folder, then the
first group in creation order with an enabled account, else the first. Rules (optional per-folder
account pins) resolve after it: `the_account_for_a_folder_is_the_projects_then_the_rules_then_the_default_selection`
(rt/projects.rs:585). Tests: core/tests/projects.rs
`a_folder_two_groups_hold_belongs_to_the_one_switched_on` (934).

**Gap.** P3: "creation order with an enabled account" is an implicit tiebreak; a folder in two
groups of the same context silently follows whichever was made first, and the person has no way
to choose. Not verified whether the window shows the losing group.

**Refactor.** Make the resolution a pure function returning the ordered candidates with the
reason each lost, used by bindings, the shell map, the Codex route and `project_context`, so all
four answer the same order with one test table.

### 4. Claude Code folder bindings and capabilities

**Target.** Every folder of a group gets, in its `.claude/settings.local.json`, the proxy route and
a capability valid for that folder and pool only; the person's own values are never touched; a
folder that leaves the group holds a capability the proxy refuses; a pool's capability can be
rotated; an off group's folders keep their route, closed (DEC-0023 D-15); the file stays out of
Git (AGENTS.md Lifecycle, Writes; CONTRACTS SB-118, SB-133, SB-140).

**Now.** rt/bindings.rs (1607 lines): capability as `ANTHROPIC_CUSTOM_HEADERS`
`X-Switchboard-Capability` (rt/bindings.rs:28-44), digest-only record `claude-bindings.json`,
`folder_capability` = folder-bound hash of the salted pool token (rt/bindings.rs:605-616;
proxy/lib.rs:168-190), `sync_salted` (rt/bindings.rs:640), salts in `pool-salts.json`
(rt/lib.rs:3401-3410), rotation `project rotate-capability` (cli/main.rs:722, 3756). 19 tests,
e.g. `a_project_folder_gets_its_pool_route_and_keeps_the_persons_settings` (871),
`a_rotated_pool_gets_its_new_capability_written_over_the_old_one` (1365),
`a_group_switched_off_keeps_its_route_and_its_switch_changes_the_fingerprint` (945),
`a_folder_bound_by_an_older_version_loses_its_auth_token_for_the_header` (1549).

**Gap.** P3: the SB-133 legacy window (`capability-migration.json`, `LEGACY_CAPABILITY_SECONDS`
7 days, rt/lib.rs:3414-3436) expired in October 2026 and is still read at every start. P3: a lost
`pool-salts.json` silently reverts rotated pools to the unsalted capability (rt/lib.rs:3399-3406),
which un-rotates them; no oplog event says so.

**Refactor.** Move salts and the migration stamp into one `capabilities.json` owned by
rt/bindings.rs; drop the legacy window; log `capability_salts_lost` when a file that existed reads
empty.

### 5. Codex in group folders

**Target.** Codex sessions in a group's folders use that group's Codex accounts through one
provider in Codex's `config.toml` pointing at the proxy, only while the switch is on; another
provider the person chose is never replaced; written at owner start, taken out at stop (AGENTS.md
Lifecycle, Writes; SB-143).

**Now.** rt/codex_route.rs (394 lines), MCP `switchboard_codex_route`. Tests
`another_provider_chosen_by_the_person_is_never_overwritten` (286),
`only_projects_with_a_codex_account_route_their_folders` (314),
`the_route_is_never_written_into_a_shell_home` (210).

**Gap.** P3: no live Codex session through a group route recorded (SB-01 covers live provider
acceptance generally). Codex OAuth renewal is Claude-only (SB-16).

**Refactor.** None needed beyond sharing the folder-to-group resolver of capability 3.

### 6. Terminals in organization folders (SB-183)

**Target.** A terminal entering an organization group's folder gets that group's Claude and Codex
homes and context; leaving restores the person's values; the hook answers in 25 ms without the
owner or store lock; zsh, bash, fish and PowerShell 5.1/7; start files written only when safe and
taken out byte for byte (CONTRACTS SB-183 nodes A-1 to A-8; AGENTS.md Lifecycle).

**Now.** rt/shell/ (7593 lines): map rt/shell/map.rs, install rt/shell/install.rs (22 tests,
e.g. `the_block_goes_in_and_comes_out_leaving_the_file_byte_identical`, 1044), scripts
rt/shell/scripts.rs (15 tests), homes rt/shell/homes.rs (`a_personal_group_gets_no_shell_home_and_no_map_row`,
826), PowerShell rt/shell/windows.rs (`a_restricted_or_allsigned_policy_reads_as_policy_blocked`,
629); CLI cli/shell.rs; budget test
`shell_env_answers_without_the_owner_or_the_store_lock_within_25_ms` (cli/tests/shell.rs:73).

**Gap.** P1 for the organization pilot: PowerShell hook never accepted on a Windows host (SB-02).
P2: SB-184 (the context profile: `GIT_CONFIG_GLOBAL`, `SSH_AUTH_SOCK`, `GH_CONFIG_DIR`, MCP
servers) not applied; SB-280 (OS-account context) waits on OBS-359. P3: SB-234 flaky zsh test
under load. P3: a personal group's folder gets no shell home (homes.rs:826), so a terminal there
runs on the ordinary sign-in while `claude` in the same folder via bindings runs on the group:
two answers for one folder, documented nowhere as intended.

**Refactor.** Decide the personal-group case in a DEC (route personal groups too, or say why
not); keep the hook scripts generated from one template per shell (core.sh already shared;
hook.ps1 is separate, 223 lines, kept equal only by `the_ps1_transition_matches_the_posix_one`).

### 7. Organizations as labels

**Target.** An organization is a label (whose work, what it is delegated for), never a lock: no
person's act is refused because of organizations; an account outside the group's organizations is
served with a warning `outside_organization`; automatic acts prefer the group's organizations and
record when they do not (`failover_outside`); balancing never lends across organizations; a domain
only suggests (DEC-0009); a team seat moves alone unless `team` is asked; the group owns the
context of a session ([DEC-0022](../../DECISIONS.md#dec-0022-an-organization-is-a-label-never-a-lock-on-where-an-account-works)).

**Now.** core/organizations.rs (1749 lines): `outside_warnings` (485-506), `groups_outside` (474),
team outcome (29-57), validation (538). Tests: core/organizations.rs
`the_persons_word_wins_then_the_provider_organization_and_a_domain_only_suggests` (1487),
`a_sessions_context_follows_its_group_else_its_account_and_the_more_private_wins` (1215);
core/tests/projects.rs `a_teammate_a_group_would_refuse_keeps_the_word_from_being_given` (443);
core/tests/rotation.rs `a_failover_outside_the_groups_organizations_is_recorded` (1565);
rt/lib.rs `an_organizations_account_moves_into_a_personal_group_with_a_warning` (9792).

**Gap.** P2: the window still tells the person the opposite of DEC-0022: the group dialog note
"Sessions in this group's folders run only on accounts of the organizations you check..."
(src/main.ts:1179, src/locales/ru.ts, SCN-070 step 1 at docs/ux/scenarios.md:1504), and
ORGANIZATION.md:284 says the group "still refuses your other accounts". P2: DEC-0022's own known
limit: an account's isolated home `homes/<account id>` is shared by every context it serves, so an
organization's Claude state and a personal group's mix there; DEC-0022 leaves it "to a board row"
and no row exists. P3: warning text is English built in core (core/organizations.rs:494-500), so
the Russian window shows it untranslated (not verified in the window).

**Refactor.** Return warnings as codes plus fields only and render text in the surfaces; key the
non-managed launch home by (account, context) with a one-time move, behind a new row.

### 8. PassionCode.ai connection (Vesta)

**Target.** Connect once in the browser (loopback, PKCE), read `GET /v1/me/organizations` at start
and at most every 6 hours, keep the last copy when unreachable and say so, mark organizations the
person left; phase 3 links a local organization to its Vesta record so its name, domains and
members stay current (SB-159, SCN-071); WF-9 walks connect, read, bind, leave and reconnect end to
end with one status line per state (SB-371).

**Now.** rt/vesta.rs (1161 lines): `connect` (336), `sync_if_due` (527), single-flight refresh
lock `TURN` (31), states `Ok|SignInAgain|ProviderSignIn|Unreachable` (47-58), read of
`/v1/me/organizations` (420). Tests: `a_browser_sign_in_reads_the_organizations_and_a_refresh_keeps_them_current`
(797), `an_organization_left_on_passioncode_is_marked_with_its_date` (861),
`two_reads_at_once_spend_one_refresh_token_each` (1056).

**Gap.** P2: SB-159 phase 3 and SB-371 open; the token's scope is `me.organizations` only
(rt/vesta.rs:22), so Vesta cannot tell Switchboard the organization's members, devices or seats;
everything an organization's admin wants on a member's machine still arrives by settings file or
ssh. P3: no end-to-end walk with evidence (SB-371).

**Refactor.** Keep vesta.rs as the only Vesta client and give it a typed read model
(organizations now; members and devices later, see capability 15) rather than adding calls ad hoc.

### 9. Organization setup checklist and seats as data

**Target.** `onboard --status` and the window name each step (owner, accounts, project, mcp,
proving call, then per organization: group, seat, hook, launch) as done, todo, needs_person or
optional; a seat step never names an address and works for an organization with several domains
or none (DEC-0022); an organization's seats are data Observatory sends to Vesta (SB-381, done); a
member's seat is one line of joining and is reversed at leaving (SB-292); actions are allowed by
access level (SB-291).

**Now.** rt/onboarding.rs (1851 lines, 21 tests): `seat_never_names_an_address` (1792),
`org_with_no_domain_has_a_seat_step` (1819), `an_organization_whose_accounts_all_live_elsewhere_asks_nothing_here`
(1331), `all_away_reads_accounts_in_machine_groups` (1671). Seats journal rt/seats.rs
(`a_grant_names_who_ordered_it_and_only_the_newest_lines_stay`, 139). `settings export --group`
(SB-292 S-6) in rt/settings_bundle.rs:210 (`scope`).

**Gap.** P2: SB-291 (access levels) and SB-292 (seat in the joining checklist, leaving) wait for the
estate's source of truth; without them every caller with the control token is the operator. P3:
SB-406 (5) the organization card shows the call-back line before an ended sign-in.

**Refactor.** None in code until SB-291 lands; then one `Authority` check at the operation layer,
not per handler.

### 10. Moving a sign-in to another machine of the person and calling it back

**Target.** `accounts send` moves a Claude or Codex subscription sign-in over ssh: marked here
first, saved there from stdin, checked there with a quota call, both undone on any failure; a lost
answer is settled by looking there; `accounts recall` takes it back with the live tokens and
proves the copy there gone; every move is a `seats.jsonl` line with who ordered it; the window,
CLI and MCP do the same (SB-283, SB-386, DEC-0013, SCN-092).

**Now.** rt/account_send.rs (2649 lines): `send_with` (276), `surrender` (653); ssh with
`BatchMode=yes`, `ConnectTimeout=15` (rt/account_send.rs:1028). 27 tests, e.g.
`a_sign_in_moves_once_with_its_tokens_on_stdin_and_this_machine_stops_using_it` (1198),
`a_lost_answer_to_the_save_is_settled_by_looking_there` (1720),
`a_recalled_sign_in_replaces_the_spent_one_and_its_copy_is_proved_gone` (2074),
`a_failed_call_back_changes_nothing_here_and_a_refused_one_goes_back` (2166).

**Gap.** P2: macOS and Linux transport only (ssh to a `switchboard` binary); a Windows machine
cannot be a target (SB-02 host missing). P3: moving from machine A to machine B is two acts
(recall, then send, DEC-0013), never one, so the sign-in passes through this machine.

**Refactor.** One `RemoteMachine` transport (prove, call, read) used by send, recall, status,
check, settings sync, usage reads and remote dispatch (today each builds `machine_proof::ssh` in
rt/lib.rs: 5359, 6855, 7086, 7307, 7334, 7354, 7501), so SB-177's hub can replace ssh in one place.

### 11. Machine groups, background reads and proof of the other machine

**Target.** Every other machine is a group of its own here with no folders, holding exactly the
accounts that live there, never routing, rotating, lending or borrowing
([DEC-0013](../../DECISIONS.md#dec-0013-an-account-on-another-machine-belongs-to-that-machines-group-and-to-no-group-here));
its accounts' usage is read every 15 minutes over ssh (20 s, three failures then hourly) and shown,
never used to route (DEC-0017); every call to another machine first proves it is the machine
trusted (SB-357); `remote status|check` name every inconsistency (`missing_there`, `held_twice`,
...); Fabric's hub replaces ssh when it carries holdings (SB-177).

**Now.** core/machines.rs (validate 100; `a_new_machine_takes_its_alias_as_name_and_a_pool_no_group_takes`,
444); core/tests/machines.rs (4 tests, `an_account_on_another_machine_leaves_every_group_here`, 52);
rt/machines.rs (`adopt` 205, `live` 346); rt/remote_usage.rs (`pass` 251, `extend_leases` 217;
`three_failures_in_a_row_read_a_machine_once_an_hour_and_a_gone_group_is_forgotten`, 501);
rt/remote_check.rs (`check` 139; `a_clean_move_finds_nothing_and_each_broken_one_is_named`, 306);
rt/machine_proof.rs (TOFU pin in `machine-keys.json`, rt/machine_proof.rs:1-14; tests 316-424).

**Gap.** P1 (trust): the proof is trust on first use, and a machine whose Switchboard is older is
let through while nothing is pinned (rt/machine_proof.rs:11-13,
`an_older_switchboard_passes_only_while_nothing_is_pinned`), so the first contact with a new
machine proves nothing; no out-of-band pin (the key is typed by the person with `machines trust`).
P2: SB-177 (hub), SB-374 (Machines page and an admin's fleet view) open. P3: backlog drift:
SB-357 reads "in review" while `machine_proof` is on main and in CHANGELOG.md:78.

**Refactor.** Pin from a signed invitation (capability 15) instead of first answer; fold
`remote_usage`, `remote_check` and `machines live` into the `RemoteMachine` reader of capability
10.

### 12. Hand-over and receive (a signed record, no credential)

**Target.** An account handed to another machine or colleague travels as a signed record without
its credential; the receiver checks the signature against the giver's published key and signs in
itself; API keys are never handed over; only on the operator's order (MCP
`switchboard_account_hand_over|receive`).

**Now.** rt/account_handover.rs: `hand_over` (109), `plan` (190); tests
`a_handed_over_account_travels_as_a_signed_record_without_its_credential` (298),
`a_received_sign_in_must_be_the_account_its_record_names` (230),
`an_api_key_is_not_handed_over` (367).

**Gap.** P3: three tests only; no test for a record replayed twice on the receiver or for an
expired record (not verified whether records expire).

**Refactor.** Treat a hand-over as an invitation of kind `seat` once capability 15 exists, so one
signed-envelope format serves both.

### 13. Delegation to a teammate's machine under a lease

**Target.** An organization's subscription serves a teammate's console sessions in one group
there, the teammate never signing in with it, revocable at any time; the teammate's machine admits
the sender only through `ssh-serve` (a forced command answering the delegation calls only); the
seat is a guest (never sent on, lent, shared, given to an agent, made the ordinary sign-in); a
48-hour lease (1 to 720) extended by each 15-minute read, switched off past it and removed 30 days
later; the teammate can keep Switchboard off their own sign-in (`own-sign-in off`); Claude Desktop
is never touched
([DEC-0025](../../DECISIONS.md#dec-0025-a-subscription-is-delegated-to-a-teammates-machine-as-a-guest-under-a-lease),
SCN-102, SCN-103).

**Now.** core/delegation.rs (826 lines): lease constants (26-34), checks on every publication
(`validate` 121, `validate_out` 149); 9 tests incl.
`a_delegated_seat_is_never_moved_shared_given_marked_or_removed_here` (577),
`past_its_lease_a_seat_is_switched_off_kept_and_removed_30_days_later` (661).
cli/ssh_serve.rs (770 lines): calls `Prove, AccountsList, ProjectList, OrgList, OrgAssign, Add,
Remove, Usage, Surrender, Lease` (cli/ssh_serve.rs:37-70), refusals before the store opens;
tests `only_the_calls_a_delegation_needs_are_answered` (446),
`the_documented_key_line_runs_through_a_shell_as_one_program` (709). rt/own_sign_in.rs
(`off_the_sources_never_read_or_write_the_persons_sign_in`, 116; `no_code_names_claude_desktop`,
152). Sender side: rt/account_send.rs
`a_delegated_seat_goes_there_as_a_guest_and_comes_back_only_to_its_sender` (2277).

**Gap.** P1: the live run S5 (a teammate's machine, a console session, Claude Desktop untouched,
recall) has never happened ([plan](../2026-11-17-org-seat-delegation/README.md) wave S5); every
claim above is from synthetic tests. P1 for the pilot: `ssh-serve` is Unix only
(cli/ssh_serve.rs:31-32) while the Appvillis pilot has Windows machines (SB-02 note). P2: setup on
the teammate's side is four manual steps including hand-editing `~/.ssh/authorized_keys`
(ORGANIZATION.md:154-188), the riskiest step for a person to get wrong (an older line without
`command=` keeps a full shell, ORGANIZATION.md:178-182). P2: an unreachable teammate keeps a live
seat for up to the lease (48 h default) and its credential for 30 days more; the only immediate
revocation is a provider sign-out, which nothing in Switchboard prompts.

**Refactor.** Generate and install the `authorized_keys` line from an invitation (capability 15)
with a check that no other line matches the key; add an Attention item "delegated seat
unreachable past N hours: sign it out at the provider".

### 14. Devices of an organization (operator addition, 2026-11-17)

**Target** ([README](README.md) "Operator's additions" 2). A machine joins an organization's
Switchboard by an invitation its owner makes, never a shared password; the organization knows its
devices (who holds each, which groups and seats are there, last seen); its accounts are shared
within the organization and moved between its devices by the owner (or an admin, SB-291) in one
act; a device leaves (its holder's act) or is removed (the owner's act) and every seat on it is
called back, with the copy there proved gone, or, when the device cannot be reached, switched off
by its lease and named for a provider sign-out. Builds on machine groups (DEC-0013), delegation
(DEC-0025) and organizations as labels (DEC-0022).

**Now (the pieces that exist).**
- Machine groups are the person's own ssh destinations, keyed by `ssh -G` identity
  (core/machines.rs), not by organization; nothing records that a machine belongs to an
  organization or to a teammate.
- Delegation (capability 13) is the only way a seat reaches someone else's machine: one group per
  ssh key line, set up by hand on the device (ORGANIZATION.md:154-188), sender key typed by the
  person; the device answers only delegation calls, so `machines --live`, the Sessions page and
  `settings sync` cannot reach it (ORGANIZATION.md:220-221).
- Trust is TOFU per ssh name (rt/machine_proof.rs:1-14); the settings signing key
  (rt/settings_signing.rs) is the only per-machine identity.
- Settings reach a device as a signed file (`settings export --group`, rt/settings_bundle.rs:210;
  verification rt/settings_signing.rs, `signed_unsigned_changed_and_other_keys_read_as_they_are`,
  207) or by `settings sync` over ssh to the person's own machines (rt/remote_check.rs:222).
- PassionCode.ai gives each member their organizations only (rt/vesta.rs:22 `me.organizations`);
  no devices, members or invitations.
- Moving a seat between two devices is recall then send through the owner's machine (DEC-0013);
  a guest can never be sent on from the device.
- Removing a device: `machines forget` works only once its group holds no account (MCP
  `switchboard_machine_set` `forget`); there is no "remove and call back everything".

**Gap.** P1, missing as a capability. (a) No invitation: no signed, expiring, single-use token
naming the organization, the group(s) and the inviter's key that a device redeems to install its
`ssh-serve` line and pin both keys; today it is a person editing `authorized_keys` and typing keys.
(b) No device registry per organization: nothing lists an organization's devices, their holders or
seats across the owner's machines; SB-177 (hub) and SB-374 (fleet view) are the open rows that
would carry it. (c) No one-act move between devices (recall plus send with a single intent line in
`seats.jsonl` and rollback). (d) No leave or remove: a device holder cannot hand every seat back
and close the entry; the owner cannot remove a device with every seat called back and the
`authorized_keys` line taken out; an unreachable device has no state beyond the lease. (e) No
access levels: who may invite or remove is the control token's holder (SB-291). (f) Windows
devices cannot join (ssh-serve Unix only). Related rows: SB-177, SB-291, SB-292, SB-374, SB-02;
no row exists for invitations or device removal: two to file.

**Refactor.** Build on what exists rather than a new channel: an `Invitation` signed with the
owner's settings signing key (same envelope as a settings file and a hand-over record), redeemed
by `switchboard org join <invitation>` on the device, which writes the restricted key line itself
(with the replace-older-line rule), pins the owner's key and answers its own key back over the
first ssh call; the owner's machine records the device as a machine group tagged with the
organization (`machines[].organization`, schema bump), so the existing reads, lease extension and
recall apply unchanged. `org device remove` = recall every `delegated_out` seat there, then an
`ssh-serve` `Close` call that removes its own line, then `machines forget`. When Fabric's hub
carries holdings (SB-177) the same envelope travels through it instead of ssh.

### 15. Settings export, import and sync

**Target.** A settings file carries groups (found again by repository), organizations, agents'
settings, rotation policies, teams, ceilings (never raised by an import), never an account or
credential; signed with the computer's key; an organization's members import the administrator's
file and can require its signer; `settings sync` carries it to another machine of the person over
ssh, signed on stdin (CONTRACTS SB-272; MCP `switchboard_settings_import|sync`).

**Now.** rt/settings_bundle.rs (1408 lines): `export` (108), `scope` (210), `plan` (351), `apply`
(518); tests `a_bundle_round_trips_and_names_no_account` (789),
`a_new_members_mac_clones_the_groups_repositories_and_gets_the_group` (1089),
`the_inbox_server_teams_hook_and_ceilings_travel_and_import_never_raises_a_ceiling` (1168),
`a_signed_file_reads_as_signed_after_the_trip_and_a_changed_one_does_not` (1377); sync
`settings_go_to_the_other_machine_signed_on_its_standard_input` (rt/remote_check.rs:405).

**Gap.** P2: an import without `signer` accepts an unsigned or foreign-signed file and reports its
state (rt/settings_bundle.rs:1400-1402); for an organization's device the signer should be pinned
by the invitation, not optional. P3: a delegation device cannot receive `settings sync`
(ORGANIZATION.md:220-221), so its group is made by hand (ORGANIZATION.md:156-157).

**Refactor.** Make the expected signer part of the device's organization record (capability 14)
and default `--signer` from it.

### 16. Balancing between groups and backup groups

**Target.** Off by default; the person chooses groups; every 15 minutes the neediest chosen group
borrows one account from a chosen group keeping two with room; loans return after 2 quiet hours or
when home needs them; one hour between moves; never across organizations, never the ordinary
CLI's account, an agent's, an elsewhere, disabled or pay-per-token account; turning off returns
every loan; a backup group lends first, keeps no minimum, never borrows
([DEC-0012](../../DECISIONS.md#dec-0012-accounts-are-lent-between-the-groups-the-person-chose-by-their-last-hours-and-days),
DEC-0016, SCN-090).

**Now.** rt/balancing.rs (1875 lines): `pass_due` (28), `signals` (203), `plan` (496), `apply`
(653), `status` (804); 22 tests incl. `no_account_crosses_an_organizations_boundary` (1359),
`accounts_that_must_never_be_lent_stay_home` (1302),
`turning_balancing_off_returns_every_lent_account` (1562),
`a_group_switched_off_and_the_pay_per_token_group_take_no_part` (911),
`a_backup_group_lends_first_and_keeps_no_minimum` (1047). Loans to and from `default` in
`Snapshot.default_shared|default_lent` (core/lib.rs:315-320). Shipped (CHANGELOG.md:259-263).

**Gap.** P3 (records only): DEC-0012 still reads "Accepted (not built yet: SB-352)" and SB-352 is
`open`, though the code and release exist. P3: no recorded observation that a live pass lent and
returned an account on the operator's machine (not verified in `balancing.jsonl`).

**Refactor.** Loans live in three shapes (a group's `shared_accounts`, `default_shared`,
`default_lent`); one `loans` list with home, borrower and since would remove the `default`
special case (core/projects.rs:635-646).

### 17. Pay-per-token group (Claude API keys under a daily ceiling)

**Target.** Every Claude API key lives in one reserved group, off until turned on with a daily
ceiling, no folder until the person adds one; the proxy checks group off, ceiling present and
ceiling reached before every request, fails closed, prices each answer at list price (unknown
model at the dearest, estimates marked); keys never leave the group, never launch outside managed
mode; a key from before outside the group is never newly selected and is named under Attention
([DEC-0023](../../DECISIONS.md#dec-0023-claude-api-keys-go-to-a-pay-per-token-group-that-starts-off-and-runs-only-under-a-daily-ceiling)
D-1 to D-19, SCN-100).

**Now.** core/projects.rs `tokens_keys_stay` (251), `set_group_off` (848),
`ensure_tokens_group_for_keys` (928), `move_key_to_tokens` (1400), publication checks
(core/projects.rs:571-597); core/lib.rs `add_routed` (1515), `upsert` (1548), `select` refusal
`KEY_OUTSIDE_TOKENS` (core/lib.rs:2511); proxy `pool_refusal` (proxy/lib.rs:606-632); day counter
proxy/tokens.rs (`a_spend_file_that_cannot_be_written_refuses_until_a_write_succeeds`, 403;
`the_day_is_loaded_from_the_spend_file_and_the_statistics_the_larger_winning`, 344); prices
proxy/prices.rs (`premiums_are_priced_high_and_marked_estimated`, 215). Core tests
core/tests/projects.rs 674-934 (6 cases). Owner makes the group at start (rt/lib.rs:3463).

**Gap.** P2: DEC-0023 D-19's known limit: the CLI and MCP without an owner may still select a key
from before until an owner has started once. P2: SB-406 edges (1) a key named for a machine group
is refused before the redirect and lost at restore, (2) `move_key_to_tokens` skips the come-home
bookkeeping for a key in a machine group, (4) pool computed outside the retry loop. P3: window
walk-through NOT_RUN ([plan](../2026-11-17-token-group/README.md) T5); SB-401 row still says
"in progress: T1 and T2" though released (CHANGELOG.md:27). P2: D-14 lets a day pass its ceiling
by the requests in flight (documented, not bounded).

**Refactor.** The rule "a Claude key lives in `tokens`" is enforced in five places (`add_routed`,
`upsert`, `move_key_to_tokens`, `select`, publication `tokens_keys_stay`); one placement function
called by all and one publication invariant would retire SB-406 (1), (2) and (4) together.

### 18. Provider keys from Observatory

**Target.** Switchboard lists the agent-usable provider keys Observatory holds by name (never a
value, never admin, BYOK, provisioning or archived slots), adds one through a pipe only
(`use_secret.py run` into the ordinary save's stdin), records the slot it came from, follows a
rotation and refuses a key marked leaked (K4); an Anthropic key goes to the pay-per-token group and
is metered; an OpenAI key waits for a ceiling that counts it
([DEC-0024](../../DECISIONS.md#dec-0024-provider-keys-come-from-observatory-by-name-through-a-pipe-and-every-launch-on-one-is-counted),
SCN-101).

**Now.** rt/observatory_keys.rs (1449 lines): `classify` (99), `list` (473), `plan` (570),
`save_step` (794), `add` (891), `OPENAI_NOT_YET` and `KEY_EXPIRED` (47, 57, 512-514); tests
`only_an_agents_provider_keys_are_listed` (1037),
`an_exported_bash_function_never_sees_the_value` (1106),
`a_text_that_may_hold_a_key_is_never_passed_on` (1163),
`an_account_beside_an_unreadable_saved_key_is_refused` (1389). Hermes metered launch:
rt/launch.rs:1922 (`metered`), test assertion rt/launch.rs:5784.

**Gap.** P2: K4 open (rotation not followed, a leaked key not refused); `observatory-keys.json`
records the slot but nothing reads it back for rotation. P2: OpenAI keys not addable (SB-409).
P3: macOS and Linux only (`UNIX_ONLY`). P3: live run with a team's Anthropic key waits for the key
(person).

**Refactor.** K4 as a background check in the same 15-minute cadence as remote reads, comparing
the recorded slot's metadata (`vault.py list`) with the saved key's record, never the value.

### 19. Agents' named keys: OpenRouter, Anthropic, OpenAI

**Target.** One OpenRouter key per machine and per group, each under a ceiling (SB-72); a key
issued for a project by Observatory lands in that project's slot without a person (SB-167); an
Anthropic key named for Hermes runs either metered through the pay-per-token group or, when the
person chooses the direct launch, labelled unmetered (DEC-0024 O-3, O-5); `launch_keys` lists only
keys a start may use, with `why_not` (O-6); an OpenAI key runs only where a ceiling counts it
(SB-409).

**Now.** rt/agent_keys.rs (`status_for` 133, spend read `the_gate_reads_a_saved_keys_spend_and_an_unanswered_read_is_unknown`,
298); core/agent_keys.rs (validation 56); rt/anthropic_keys.rs (`set` 111;
`a_file_of_names_never_holds_a_value_and_reads_back`, 279); rt/launch_keys.rs (`list` 21;
`the_pay_per_token_groups_keys_are_offered_with_why_not`, 195); MCP
`switchboard_openrouter_status|model`.

**Gap.** P1 (money): a named Anthropic key for Hermes (`--anthropic`, SB-390) is spent with no
ceiling at all; DEC-0024 only labels it `metered: false`. P2: SB-167 (delivered keys not picked up)
and SB-409 (OpenAI) open. P3: two key stores for one provider (Anthropic as an account in the
pay-per-token group and as a named key in `anthropic-keys.json`) can hold one value twice.

**Refactor.** Route the direct Anthropic launch through the pay-per-token group's proxy as O-5
already does for group launches, then retire the unmetered path or give named keys a ceiling of
their own; one `ProviderKey` record with targets (account, agent) instead of two stores.

### 20. Spending ceilings

**Target.** Daily ceilings for the machine, for a group, and per task since its first paid
hand-over; a reached ceiling is a question (double or keep), never a silent wall; raising, clearing
or doubling needs the window or the operator's own words (SB-299); an unreadable spend file hands
nothing over; per-project ceilings come from the grant registry (SB-96) and usage through the proxy
carries the grant (SB-97); a paid start cannot escape a group's ceiling by moving its folder
(SB-216) (CONTRACTS "Spending ceilings (SB-72, 0.7.0)").

**Now.** rt/spend.rs (2857 lines): file and caps (21-48), `Surface::loosens` window or operator
(rt/spend.rs:162-175), `record_loosening` (225), `NO_OWN_KEY` group ceiling needs the group's own
OpenRouter key (rt/spend.rs:61, 766) except the pay-per-token group; gate in `fallback::spend_gate`
(rt/spend.rs:13-14). 28 tests incl. `an_unreadable_spend_file_hands_nothing_over_never_no_ceiling`
(1512), `double_is_refused_from_mcp_and_cli` (1458),
`two_surfaces_setting_one_ceiling_at_once_never_loosen_it` (2336),
`a_key_save_racing_the_groups_removal_leaves_no_key_without_its_group` (2787).

**Gap.** P1: SB-96 (a chain may pin another project's key, so one project's work runs on
another's money) waits for Observatory's grant registry; SB-72 itself still `open`. P2: two meters
that do not meet: OpenRouter ceilings read the key's provider-side usage per start, the
pay-per-token ceiling counts list price per request in the proxy (proxy/tokens.rs); a machine
ceiling does not see the pay-per-token group's spend (not verified; no test names it). P2: SB-216
(folder moved out of a capped group escapes its ceiling), SB-97 (per-grant metering), SB-233.
P3: refusal sentences still say "done in the Switchboard window" (`RAISE_IN_WINDOW`, `DOUBLE_IN_WINDOW`,
rt/spend.rs:65, 90) although an agent with the operator's words may do it (SB-299), contrary to the
operator's "the operator presses nothing". P3: SB-229, SB-230 window leftovers.

**Refactor.** One spend ledger (`spend.rs` owns reading both meters) so the machine ceiling sums
OpenRouter usage and proxy list-price spend; refusals name the MCP route with `decision` beside the
window.

---

## Cross-cutting refactors for this area

- **One account placement record.** `Snapshot` keeps nine parallel per-account maps (`elsewhere`,
  `locations`, `assigned_to`, `memberships`, `also_organizations`, `returns_to`, `delegated`,
  `delegated_out`, plus `default_shared|default_lent`; core/lib.rs:291-341). Each rule (guests,
  machines, keys, loans) re-derives placement from them. A `Placement { home, shares, loan,
  elsewhere, delegation, given_to, organizations }` per account, validated once, would collapse
  the five key-placement checks of capability 17 and the guest checks of capability 13.
- **Schema version from a feature table.** `schema_version` is a 16-step ladder of `if`s
  (core/persistence.rs:24-80); a table of (version, predicate) checked in order, with a test that
  every field added since 4 has a row, keeps readers safe without the hand ladder.
- **Split the runtime operation layer.** rt/lib.rs is 15,654 lines with about 123 `Operation`
  variants in one enum (rt/lib.rs:121); this area's 45 or so (projects, organizations, Vesta,
  machines, seats, keys, settings) move into per-domain handler modules with their tests
  (detail in [05](05-code-structure.md)).
- **One `RemoteMachine` transport** (capabilities 10, 11, 14, 15).
- **Codes, not English, from core** (capability 7).

## Records to correct (no code)

SB-352, SB-357 and SB-401 rows read open or in review though merged and released (CHANGELOG.md:27,
78, 259); DEC-0012 status "not built yet"; DEC-0007 "work in" half; rt/suggest.rs:10-12; SCN-070
step 1 and src/main.ts:1179; ORGANIZATION.md:284 (and its Russian twin). New rows to file: device
invitations and device removal (capability 14); per-(account, context) isolated homes (DEC-0022
limit, capability 7); unmetered named Anthropic key (capability 19).

## Summary

| # | Capability | State | Gap severity | Backlog ids |
|---|---|---|---|---|
| 1 | Groups and their folders | done | P2 | SB-07, SB-362 |
| 2 | Folder discovery | partial | P2 | SB-141, SB-391, SB-234 |
| 3 | Which group serves a folder | partial | P3 | (new) |
| 4 | Claude Code folder bindings | done | P3 | (none) |
| 5 | Codex in group folders | done | P3 | SB-01, SB-16 |
| 6 | Terminal hook (SB-183) | partial | P1 | SB-02, SB-184, SB-280, SB-234 |
| 7 | Organizations as labels | partial | P2 | SB-403, (new) |
| 8 | PassionCode.ai connection | partial | P2 | SB-159, SB-371 |
| 9 | Organization checklist and seats as data | partial | P2 | SB-291, SB-292, SB-406 |
| 10 | Send and recall to the person's machines | done | P2 | SB-02 |
| 11 | Machine groups, reads, proof | partial | P1 | SB-357, SB-177, SB-374 |
| 12 | Hand-over and receive | done | P3 | (none) |
| 13 | Delegation to a teammate's machine | partial | P1 | SB-403 (S5), SB-02 |
| 14 | Devices of an organization | missing | P1 | SB-177, SB-291, SB-292, SB-374, (new x2) |
| 15 | Settings export, import, sync | partial | P2 | SB-292 |
| 16 | Balancing and backup groups | done | P3 | SB-352 (record) |
| 17 | Pay-per-token group | partial | P2 | SB-401, SB-406 |
| 18 | Provider keys from Observatory | partial | P2 | SB-404 (K4), SB-409 |
| 19 | Agents' named keys | partial | P1 | SB-167, SB-409, SB-390, (new) |
| 20 | Spending ceilings | partial | P1 | SB-72, SB-96, SB-97, SB-216, SB-229, SB-230, SB-233 |

By state: done 6, partial 13, broken 0, missing 1.
