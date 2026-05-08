---
number: 0005
title: Code style — current intended state only
status: active
issue: "#16"
sources: ["CLAUDE.md#code-style", "~/.claude/CLAUDE.md#code-style"]
tags: [convention, style]
---

# CDR-0005: Code style — current intended state only

## Context

- Change history lives in git. Encoding it in code (comments like `// use this instead of X`, names like `createPlayersWithNames` when `createPlayers` is correct) creates rot the moment the surrounding code shifts.
- Forward-looking commentary (TODOs naming a hypothetical future, dead code kept "for later") creates the same problem in the other direction.

## Decision

- Code reflects the **current, intended state**. Nothing else.
- No history-encoding comments. Examples to avoid:
  - `// previously used X, now uses Y`
  - `// removed: <feature>`
  - `// moved from foo.rs`
- No history-encoding names. If `createPlayers` is the right name, that's the name; no `createPlayersWithNames` just because a different `createPlayers` once existed.
- No "for later" stubs. Add the code when it's needed.
- Surgical changes:
  - Touch only what the task requires.
  - Don't refactor adjacent code that isn't broken.
  - Match existing style, even if you'd do it differently.
  - Mention unrelated dead code; don't delete it unless asked.
  - Remove imports / variables / functions your changes orphaned.
- Comments earn their place. Default: no comment. Add one when the **why** is non-obvious: a hidden constraint, a subtle invariant, a workaround for a specific bug, behaviour that would surprise a reader.
- No "what" comments; well-named identifiers do that job.
- No PR-context comments (`// added for the X flow`, `// fixes #123`); they belong in the commit, not the code.
- Naming reflects the type when not obvious from immediate context. Descriptive over terse, even where Rust convention permits short names. `payload` beats `p`. `scope_store` / `pending_scope_store` beats `scopes` / `pending`.
- Short names (`s`, `v`, `i`) are fine only when the variable's whole use is constrained to its declaration line (or at most ~3 lines).

## Consequences

- New readers see a clean, current model. No mental rewind through "this was once different" comments.
- Diffs are smaller because adjacent cleanup doesn't ride along.
- Some "obvious" comments get removed during review. That's fine; git tells the history.

## Compliance (review agents)

- Comments referencing prior implementations or commit context are a flag.
- Names encoding history (`*New`, `*V2`, `legacy*`, `*WithSomething` when the unmarked name was just removed) are a flag.
- `// what` comments restating obvious code are a flag.
- Adjacent refactors not required by the task are a flag; surgical changes only.
- Stubs / scaffolding for unbuilt features are a flag; build when needed, not before.
- Imports / variables / functions orphaned by a change but left in the diff are a flag.
