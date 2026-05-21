---
name: playback
description: User explains code back to the agent, file by file. Agent verifies understanding, corrects misconceptions, accepts challenges to design decisions, and makes on-the-fly improvements. Builds deep comprehension while improving the codebase.
disable-model-invocation: false
---

# playback

**The user explains code to the agent.** The agent listens, verifies, corrects, and improves. Understanding and codebase quality improve together.

## Triggers

`/playback`, "let me explain this back to you", "I'll walk you through this", "listen while I explain"

---

## Inputs

1. **Source** — what to play back:
   - A branch, commit range, PR, or set of files.
   - Or: "the recent work on X" (agent resolves).
2. **Resolution** (optional) — same scale as `/reconstruct`:
   - `fine` — file-by-file, near line-by-line.
   - `standard` (default) — one concept/module at a time.
   - `coarse` — broad strokes, high-level only.

---

## Setup

### Load ubiquitous language

If the repo defines domain context docs (`CONTEXT-MAP.md` at root, per-package `CONTEXT.md` files), read them. This is the terminology reference for the session. When the user uses a wrong or ambiguous term, correct gently using this source.

### Suggest a layer order

Before the user starts explaining, propose a reading order:

```
1. <file/module> — <one-line role>
2. <file/module> — <one-line role>
...
```

Same ordering principles as `/reconstruct`: foundations first, dependents later. The user may reorder or skip.

### Set expectations

Tell the user:

- Explain in your own words. Voice-to-text is fine — don't worry about polish.
- Challenge anything that looks wrong. The agent will evaluate honestly.
- After each explanation block, the agent responds with verification + corrections + any live fixes.

---

## Loop

### User's turn

The user explains one layer/file/concept in their own words (may be long, voice-to-text). They may:

- Narrate what the code does and why.
- Challenge design decisions ("why not X instead?").
- Flag confusion or propose alternatives.

### Agent's response

After each user message:

1. **Verify** — confirm what's correct. Be specific, not just "yes."
2. **Correct** — flag misconceptions, missed nuances, conflated concepts.
3. **Language** — gently correct terminology that deviates from ubiquitous language. If the repo has a `ubiquitous-language.md` or equivalent, load it at setup. Flag where the user's phrasing reveals genuine ambiguity in the codebase's naming.
4. **Challenges** — for each design decision the user pushed back on:
   - Valid: agree, make the fix immediately, show the diff.
   - Sound: explain the reasoning and constraints the user missed.
   - Judgement call: present both sides honestly.
5. **Suggest next** — propose the next file/concept, or ask if staying on the current one.

### Live fixes

When a challenge leads to improvement:

- Change immediately. Don't defer.
- Show diff and reasoning.
- Verify green (type-check, lint, tests).
- Real commits, not hypotheticals.

---

## Rules

- **Listen first.** Respond after they finish, not mid-explanation.
- **Be honest.** Wrong understanding gets clear correction, not softened ambiguity.
- **Accept valid challenges.** The user may spot real problems. Act on them.
- **Don't over-correct.** Imprecise phrasing with correct understanding = confirm, don't nitpick.
- **Track comprehension.** Struggle points signal code clarity issues, not user issues.
- **No fabrication.** Only explain what the code does. Say "unsure" when unsure.
- **Pause is valid.** Stop at any layer. No pressure to complete.
