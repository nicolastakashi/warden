# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@AGENTS.md

## Claude Code specifics

The shared architecture, commands, and rule schema live in `AGENTS.md` (imported
above). The notes below are only relevant when operating *as Claude Code*.

- **Authoring rules — use the skills.** `warden-rule-author` writes a single rule
  as a valid `rules/*.yaml` (it owns the closed schema). `warden-rule-discovery`
  investigates a codebase to propose *which* rules it needs, grounded in evidence,
  and **confirms intent with a human before committing** (static signals mislead
  about intent). Reach for these instead of hand-writing rules. They follow the
  open [Agent Skills](https://agentskills.io) format and live in `skills/`;
  `.claude/skills` is a symlink to it so Claude Code discovers them.

- **Runtime gate as a hook.** Wire `warden gate` as a `PreToolUse` hook in
  `.claude/settings.json` (matcher `Write|Edit`); it reads the hook payload on
  stdin. Blocking semantics are in the runtime-gate section above — JSON
  `permissionDecision: "deny"` + exit 0, never exit codes. Template:
  `.claude/settings.example.json`; live demo wiring: `demo/.claude/settings.json`.
