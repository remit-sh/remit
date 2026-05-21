---
name: reconstruct
description: Reviewer-driven iterative reconstruction — works through a landed change one layer at a time, proposing the next step fresh from the remaining diff, calibrated to the reviewer's pace and comprehension.
disable-model-invocation: false
---

# reconstruct

**Iteratively reconstruct a landed change, one layer at a time.** Each step is proposed fresh from the remaining diff — not pre-planned. The agent calibrates chunk size and explanation depth to the reviewer's pace.

## Triggers

`/reconstruct`, "reconstruct this", "layer this out", "rebuild layer by layer", "walk me through this"

---

## Inputs

The user provides:

1. **Source** — what to reconstruct:
   - Commit range: `abc123..def456`
   - Branch: `spike/some-feature`
   - PR: `#123`
   - Recent commits: "last 5 commits on main"
2. **Mode** — how to output:
   - `branch` (default) — real commits on a reconstruction branch, one per layer.
   - `concept` — numbered files in a reference folder. Executable but not production code.
3. **Base** (optional) — branch to reconstruct on top of. Defaults to `main`.

If not supplied, ask. Mode defaults to `branch`.

4. **Resolution** (optional) — how granular to go:
   - `fine` — near line-by-line, small layers, full explanation of each concept.
   - `standard` (default) — one concept per layer, moderate explanation.
   - `coarse` — large chunks, minimal explanation, faster pass.

   Established upfront but may shift mid-session. If the reviewer starts asking more questions, tighten resolution. If they're breezing through, loosen it.

---

## Loop

### Step 0 — Read the source

1. Gather the full diff of the source.
2. Read associated docs (TNs, issue body, PR description) for intent and vocabulary.
3. Form an internal understanding of the concepts introduced.

### Step 1 — Propose the next layer

Look at the **remaining diff** (source minus what's already been reconstructed). Propose one layer:

```
Next: <name> — <one-line description>

Why this comes next: <reasoning — what it builds on, what it unblocks>
```

**Calibration:**

- Start at the declared resolution. Adapt as the session unfolds.
- **Tighten** (smaller chunks, more explanation) when the reviewer explores, asks "why", or corrects.
- **Loosen** (bigger chunks, less narration) when accepting without questions or explicitly requesting speed.
- If the reviewer says "can we do X next?" — follow their lead.

**Ordering principles (inform proposals, don't rigidly dictate):**

- Foundations before dependents.
- Infrastructure before domain. Domain before composition.
- One concept per layer. A concept: a type + its collaborators, a pattern, a structural move, a vocabulary change.
- Each layer compiles independently — no forward references.

### Step 2 — Implement the layer

Once confirmed (or redirected):

1. **Explain** — what this layer introduces and why. Teach the concept, not just the code.
2. **Implement** — write code/docs for this layer only.
   - `branch` mode: commit as `reconstruct(<scope>): L<N> — <layer name>`
   - `concept` mode: numbered file in the concept folder.
3. **Verify** — type-check, lint, tests. Must be green.
4. **Present** — show what landed. Explain non-obvious choices.

### Step 3 — Wait for the reviewer

- **Accept** — next iteration (back to Step 1).
- **Ask questions** — explain further. Educational, not mechanical.
- **Request changes** — amend before proceeding.
- **Redirect** — "let's do X next" or "merge these two concerns."
- **Stop** — valid at any point. Branch/folder is coherent at every step.

Never advance until the reviewer signals readiness.

### Step 4 — Repeat or finish

When the remaining diff is empty (or the reviewer says "that's enough"):

- `branch` mode: report branch name, commit count, comparison to original.
- `concept` mode: add a `README.md` listing layers. Commit.

---

## Layer commit format (branch mode)

```
reconstruct(<scope>): L<N> — <layer name>

<one-paragraph description of what this layer introduces and why>
```

Scope inferred from source (package name, feature, or "all").

---

## Concept folder format (concept mode)

```
<target>/concept/
  README.md              # layer index with descriptions
  01-<layer-name>.ts     # layer 1
  02-<layer-name>.ts     # layer 2
  ...
```

Files are self-contained. Import from prior layers or stub inline. Inspection ergonomics over modularity.

---

## Rules

- **One step at a time.** Each proposal emerges from the remaining diff and the reviewer's pace.
- **Adapt.** Chunk size and explanation depth flex with reviewer engagement.
- **Each layer is green.** Compiles, lints, tests pass. No "we'll fix this later."
- **Teach, don't just replay.** Explain design decisions, trade-offs, alternatives rejected.
- **Reviewer drives direction.** Agent proposes; reviewer decides what's next.
- **No behaviour change.** Restructures presentation, not semantics. Divergence is a separate task.
- **Vocabulary follows the source.** Don't rename unless requested.
- **Pause is valid.** Branch/folder is coherent at every step.
