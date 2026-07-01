---
name: warden-rule-discovery
description: >-
  Discover which Warden rules a codebase needs by investigating it — the
  upstream step before any rule gets written. Use this whenever someone wants to
  set up, bootstrap, or onboard the warden on a project, asks "what rules should
  this repo have?", points the warden at a codebase that has zero rules, or
  wants to turn a team's conventions into enforceable policy. Trigger when the
  job is figuring out WHICH rules to create — as opposed to writing one already-
  decided rule, which is the sibling skill `warden-rule-author`. It surveys the
  repo, proposes a prioritized, evidence-backed rule set, confirms the real
  intent with a maintainer, then hands each confirmed rule to the author skill.
compatibility: "Requires the warden CLI; llm-rule dry-runs also need the claude CLI."
---

# Discovering Warden rules for a codebase

This is the **discover** half of a two-skill pipeline:

```
warden-rule-discovery   →   warden-rule-author
"which rules does this        "write this one rule
 repo need, and why?"          as valid rules/*.yaml"
```

Your job here is to investigate a codebase and come back with a **prioritized,
evidence-backed proposal** of rules — then, once a human confirms the intent,
hand each rule to `warden-rule-author` to write the YAML and validate it. Do
not write rule files yourself from this skill; producing the schema is the
author skill's job.

## The cardinal rule: propose and confirm, never guess the intent

Static signals tell you *what the code does*, not *what the team considers
wrong*. These come apart constantly. The most important thing this skill teaches
is to **surface candidate anti-patterns and ask a maintainer before committing
to any rule.**

> Real example this skill is built from: an agent saw `os.getenv` all over a
> repo and proposed a "don't read env vars" rule. Plausible — and wrong. The
> team's actual anti-pattern was gating features on `cluster.id == "..."` /
> `cluster.env_type == "..."` instead of feature flags; the env reads were
> mostly legitimate boundary config. Only the maintainer's domain knowledge
> caught it. Grepping found the *symptom population*; the human supplied the
> *intent*.

So: investigate to form hypotheses, bring evidence, and let the human confirm,
correct, or kill each one. A wrong rule that ships erodes trust in the whole
gate faster than a missing one.

## Investigation workflow

Work top-down — cheapest, highest-signal sources first.

1. **Read the team's own docs first.** `AGENTS.md`, `CLAUDE.md`, `CONTRIBUTING`,
   `DEVELOPMENT.md`, `docs/`, ADRs. Documented conventions ("prefer X over Y",
   "features must not import each other", "never log above debug in rendering
   paths") are the richest, highest-intent source of rules — the team already
   wrote down what they care about. Each "must / never / prefer / always"
   sentence is a candidate rule.
2. **Read the lint/CI config.** `.flake8`, `ruff.toml`/`pyproject`, `mypy.ini`,
   `.pre-commit-config.yaml`, CI workflows. These encode conventions already
   enforced — don't duplicate them; look for the gaps they *don't* cover (lint
   catches style; the warden is for architecture, security, and team-specific
   semantics).
3. **Map the structure.** Top-level packages, module boundaries, where the
   "features"/"services"/"domains" live. Layering and "X must not depend on Y"
   boundaries become `structural` forbidden-import rules.
4. **Grep for each candidate — for two distinct reasons.** (a) *Confirm* the
   convention is real in the code (does the anti-pattern actually occur?), and
   (b) *measure its extent* (how many hits, in which paths). Extent drives
   enforcement: a convention violated in 200 places can't ship as `block` on
   day one. Grep the preferred pattern too — seeing the "right way" used widely
   confirms the convention is live.
5. **Anchor every candidate in evidence + a stated convention.** For each one,
   you should be able to say: "AGENTS.md says X; here are N occurrences of the
   violation at these paths; the sanctioned alternative is Y." If you can't
   point to a convention AND evidence, it's a generic rule — drop it.
6. **Confirm with a maintainer** (the cardinal rule above). Present the
   candidates, your evidence, and your uncertainty. Ask which are real, whether
   you've identified the right anti-pattern, and what the sanctioned alternative
   is. Expect to be corrected — that's the point.

## Calibrate conservatively

A zero-rules repo should not wake up to a wall of blocking failures. Defaults:

- **Enforcement:** start at `warn` for real conventions, `audit` for "observe
  adoption / not sure yet". Reserve `block` for things the team explicitly wants
  to stop, and usually only after the existing violations are cleaned up.
- **Weight:** `4` critical, `2` moderate, `1` minor — for the CI score.
- **Scope with `paths`.** If a pattern/llm rule would be noisy repo-wide because
  the convention only holds in certain areas (e.g. "no elevated logging *in
  rendering paths*", "no env gating *in feature code*"), scope it with `paths`
  rather than dropping to `audit` to hide the noise. Boundary/infra code often
  legitimately does what's forbidden in feature code.
- **Match type:** literal token/call → `pattern`; import boundary → `structural`
  (Python only); needs judgment → `llm` (and never repo-wide — scope it).

## Output — the proposal

Present a prioritized table the maintainer can react to. For each candidate:

| field | what to put |
|---|---|
| intent | the convention in one line ("gate features with flags, not env checks") |
| evidence | the doc that states it + grep extent ("AGENTS.md; 38 hits under features/") |
| match type | pattern / structural / query / llm, with the concrete patterns, globs, or `.scm` |
| enforcement / weight | with the conservative default and your reasoning |
| paths | the scope, if narrower than the whole repo |
| confidence | high / needs-confirmation — flag the ones you're guessing at |

Lead with the highest-confidence, highest-value rules. Mark the speculative ones
clearly so the human knows where to focus their correction.

## Close the loop

For each rule the maintainer confirms:

1. Hand it to **`warden-rule-author`** to produce the `rules/*.yaml` (it owns
   the schema, ids, enforcement/scope/weight/paths, and the closed-schema rules).
2. Run `warden validate --rules <dir>` to prove it parses.
3. **Dry-run** `warden check <path> --rules <dir> --no-llm` and confirm the rule
   fires on the real violations you found (and *not* on the legitimate cases you
   scoped out). A rule that validates but doesn't fire where expected — or fires
   on legit code — goes back for refinement before you call it done.

Report back the rules created, where they fired, and the resulting score — and
which candidates were dropped or deferred, so the investigation is auditable.

## Anti-patterns in your own work

- **Generic rules.** "No `print()`", "no `TODO`" with no tie to this team's
  conventions are noise. Every rule must trace to something the team stated.
- **Blanket `block` on a populated violation.** It breaks the build on day one
  and gets the warden disabled. Surface first, block later.
- **Guessing the anti-pattern from a symptom.** The whole reason this skill
  exists — see the cardinal rule. Bring it to the human.
