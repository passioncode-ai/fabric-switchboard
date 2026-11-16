# Restart waves: what Claude Code does on a resume (SB-397 R1.0, 2026-11-16)

Measured on the MacBook, Claude Code `2.1.299` (`claude --version`).

## A resumed session keeps its id

Every live `claude` process started with `--resume <id>` was compared with the session file its
pid writes (`<config>/sessions/<pid>.json`, field `sessionId`), in `~/.claude` and in Switchboard's
`homes/*` and `runtimes/*`:

```
pid=23779 resume=090405e6-9f6e-43d4-b8b3-1b6dc28b119b file=090405e6-9f6e-43d4-b8b3-1b6dc28b119b SAME
pid=89012 resume=acb6bc7a-6c50-4499-b58e-580d8c3d6045 file=acb6bc7a-6c50-4499-b58e-580d8c3d6045 SAME
pid=26762 resume=1eae20cd-22b1-40a9-8094-24fc40e86237 file=1eae20cd-22b1-40a9-8094-24fc40e86237 SAME
```

So a wave's `back` state reads the same session id under a new pid (`restarts::advance`), and R-6 of
the [spec](../plans/2026-11-16-session-service/README.md) holds for this version. A later Claude
Code that changed it would show as `failed: not_back` for every session of a wave: re-run this check
after a Claude Code update that touches `--resume`.

## Not measured yet

- Which of a new skill, a changed skill, a plugin, a hook or an MCP server a running session picks
  up without a restart. A wave restarts every listed session regardless; this only decides how often
  a wave is needed.
- Whether `Stop` fires when the person interrupts a turn. R1 does not depend on it: a turn's end is
  read from the session file's `status` (`idle`).
