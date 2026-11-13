# The agents database's architect: Switchboard

North Star §6 «Merged with the North Star, validated by the architect» (operator, 2026-11-13).

- **2026-11-13, scan.** `python3 tools/audit.py scan ~/DATA/fabric-switchboard` from fabric-agents-database
  `2ea5dd1`, on this repository at `1892a74d`: [2026-11-13-scan.json](2026-11-13-scan.json). No template fits
  closely (the first three: `delivery-gateway` 5.316, `web-quality` 5.265, `data-room` 4.966; text similarity at
  most 0.343). Switchboard is an account service, not an agent built from a template.
- **The judged half** (`audit.py start`, judged by `playbooks/audit-agent.md`) writes its folder in the
  fabric-agents-database repository, so its owner runs it in its G30 pass over this repository's `main`; the
  verdict is kept here beside the scan. No release ships past a blocking gap it names ([SB-350](../backlog.md)).
