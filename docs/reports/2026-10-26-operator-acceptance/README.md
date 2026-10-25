---
report:
  id: fabric-switchboard/2026-10-26-operator-acceptance
  title: "Operator acceptance after 0.6.83: three live checks before the first organization"
  kind: checklist
  project: fabric-switchboard
  domains: [acceptance, organizations, reliability]
  as_of: 2026-10-26
  status: active
  valid_until: 2026-11-14
  summary: >-
    What the operator does, at their signal after 0.6.83 (decision D-8 of 2026-10-25), and what the agent
    does around it: connect PassionCode.ai (SB-159), the two-machine renewal check with a spare Claude
    account (SB-178), and a day of owner timing on the installed 0.6.83 (SB-174). Each step names the
    command, what to look at and where the result is recorded.
  sources:
    - {name: "Plan of 2026-10-25", path: "docs/reports/2026-10-25-audit-and-plan/README.md", read_at: 2026-10-26}
    - {name: "Board", path: "docs/evidence/backlog.md", read_at: 2026-10-26}
    - {name: "Two machines", path: "docs/research/two-machines-2026-10-23.md", read_at: 2026-10-26}
---

# Operator acceptance after 0.6.83

Decision D-8 of 2026-10-25: these checks run after 0.6.83, at the operator's signal. Each one needs a
person (a sign-in, a second machine). The agent prepares, watches and records; the operator only does
the steps marked **you**.

## 1. Connect PassionCode.ai (SB-159)

Built in 0.6.66, never checked live.

1. **You:** in a terminal, `switchboard org connect`. A browser page of `org.passioncode.ai` asks you
   to sign in and allow *Switchboard* to read your organizations. Allow it within 5 minutes.
2. The agent then reads `switchboard --json org passioncode` and `switchboard --json org list`:
   the state is `ok`, and the PassionCode.ai organization appears with its domains.
3. Recorded on SB-159 with the time and the state; the row closes.

If the page says the app is unknown or the state reads `provider_sign_in`, nothing is wrong on your
side: the agent files it with Vesta (board) and the row stays open.

## 2. Two machines, one spare Claude account (SB-178)

Decision D-6 of [two machines](../../research/two-machines-2026-10-23.md): before the two Macs share
accounts (RM-21 S1), measure whether renewing an account on one signs it out on the other.

1. **You:** pick a spare Claude account (not one you work on). Sign it in to Switchboard on the
   Mac Studio and on the MacBook (`switchboard login begin --provider claude --label spare-check`, then
   `switchboard login finish LOGIN_UUID` once the browser sign-in is done).
2. The agent, on the Mac Studio: reads the account's quota once on each machine
   (`switchboard usage ACCOUNT_UUID`), so both have a working token.
3. The agent renews the account on the Mac Studio only (it reaches its renewal on its own within the
   token's life; a quota check after the token expired asks for it).
4. **You:** on the MacBook, run `switchboard usage ACCOUNT_UUID` once.
5. Result: a fresh number means the two machines can share an account; `sign_in_required` or a 401
   means a renewal on one machine signs the other out, and RM-21 S1 keeps one machine per account
   (D-2 stays). Recorded in `docs/evidence/` and on SB-178.

## 3. A day of owner timing on 0.6.83 (SB-174)

No person needed; listed so the result is read before the first organization.

1. The agent confirms the installed app is 0.6.83 (`switchboard --json status`, `owner_version`).
2. After 24 hours it counts `control_slow`, `vault_slow` and `transaction_held` in
   `~/Library/Logs/Fabric Switchboard/switchboard.log` and compares with 2026-10-25 (`snapshot`,
   `recovery`, `sessions` 5 to 52 s at load 89 to 100).
3. Result on SB-174 and SB-163: closed when no read operation waits more than 5 s outside a
   transaction; otherwise the next cause, with its numbers.
