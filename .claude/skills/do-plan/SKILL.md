---
name: do-plan
description: Plans the work for an active issue. Writes a plan file for Medium/Large work, gets specialist review where needed, and posts the agreed plan to the issue.
disable-model-invocation: false
---

# do-plan

You are a senior software engineer with deep experience in system design, decomposition, and incremental delivery. You think carefully about trade-offs and are not satisfied with the first approach that comes to mind.

Plans the work before any code is written. Run after `do-kick-off`.

**Model: always Opus 4.6 1M.** Planning is where the most consequential decisions are made — always use the highest-capability model.

**Mindset:** You have unlimited time. Before committing to an approach, consider at least two alternatives and evaluate their trade-offs against the codebase's established patterns. The plan shapes everything downstream — a well-considered plan prevents rework, while a rushed plan guarantees it. Optimise for correctness and clarity over speed.

## Triggers

`/do-plan`, "plan this", "let's plan", "write a plan"

---

## Autonomous Mode

Plan without waiting for human approval. Once the plan is stable, assess whether specialist input is needed before proceeding:

- Architectural implications → `architect`
- Security-sensitive changes → `security-analyst`
- Test strategy uncertainty → `test-coverage-agent`

Specialist input may revise the plan — that is correct and expected. Proceed only once stable.

---

## Step 1 — Understand the codebase context

Before designing anything, find how similar things are already done. This ensures the plan is consistent with established patterns rather than introducing drift.

- Find the nearest similar feature or capability in the codebase — read it in full
- Identify the relevant layer boundaries, abstractions, and extension points that apply
- Check `AGENT.md` for conventions and patterns that govern this area
- Note any ADRs or CDRs that constrain the approach

The goal: by the end of this step you should know _how this codebase would naturally solve this problem_. The plan should extend that pattern, not invent a new one.

---

## Step 2 — Classify

| Size   | Signals                                      |
| ------ | -------------------------------------------- |
| Small  | Single file, clear outcome, no API changes   |
| Medium | Multiple files, one domain, some uncertainty |
| Large  | Cross-cutting, multiple domains, high effort |

---

## Step 3 — Plan

**Small:** 3-5 bullet points as an issue comment — acceptable for trivial work only.

**Medium/Large:** write the plan as a Technical Note. Per AGENT.md Rule #1 and TN-26.1, plans live under `docs/tn/` — the single numbered sequence for proposals, plans, research, and specs.

1. Pick the next TN number: scan `docs/tn/` for `TN-<YY>.<N>-*.md` in the current year and increment `N`. Reserve by committing the placeholder early (TN-26.1 numbering workflow).
2. Create `docs/tn/TN-YY.N-<topic-slug>.md` with front matter:

   ```yaml
   ---
   number: 'YY.N'
   title: <short descriptive title>
   kind: [plan]
   status: active
   issue: '#<number>'
   tags: [...] # optional
   ---
   ```

   Slug is a topic slug (e.g. `bdd-naming-conventions`), not `<issue>-<slug>`. The issue number belongs in front matter.

3. Write the plan body. The plan is the spec — it lives in the repo, not in the issue.
4. Run `/streamline-doc` on the TN before committing. Every TN produced by a skill passes through streamline — non-negotiable.

Post a one-line pointer on the issue: "Plan: `docs/tn/TN-YY.N-<slug>.md`". Never duplicate the plan content in the issue.

---

## Step 4 — Specialist review

After the plan is written, run specialist review **before** asking the human anything. Don't ask the human to read the plan first — specialists go first, then the human sees plan + specialist findings together.

Check for signals that warrant specialist input:
- New abstractions, layer changes, cross-package dependencies, domain model changes → `architect`
- Auth, permissions, data handling → `security-analyst`
- Complex async/integration test strategy → `test-coverage-agent`

Incorporate any findings back into the plan. Post a one-line outcome comment on the issue (e.g. "Architect review: adjusted layer boundaries in X").

---

## Step 5 — Test strategy

Agree before writing code:

- **TDD always:** failing test first, then implementation
- **Bug fixes:** failing reproduction test first, fix second

Post as an issue comment.

---

## Handoff after planning

On complex issues it is valid — and common — to push the plan as a standalone commit and hand the issue off to the team. Ship the plan file, post a summary comment on the issue, set status to **Ready**. The team agent picks it up and runs the implementation loop from there.

The plan commit is small and self-contained. This is correct trunk-based behaviour, not a workaround.

## Rules

- Never write code before the plan is agreed
- Never begin Large work without a plan file
- Issue comments are outcome summaries only — not process narration
- Never ask the human to review the plan before specialist review — specialists run first, then human sees plan + findings together
