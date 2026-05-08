---
number: 2026-001
title: Technical Notes and documentation structure
kind: proposal
status: superseded
issue: "#6"
superseded-by: [CDR-0004]
tags: [documentation, process, meta]
---

# TN-2026-001: Technical Notes and documentation structure

> Superseded by [CDR-0004](../decisions/cdr/CDR-0004-doc-system-layout.md). The CDR is now authoritative; this TN is kept for historical context.

> Eats its own dogfood: this is the first TN, in the new location, in the new format.

## Style — applies to every TN

- Compact. Bulleted. Get to the point.
- No long prose paragraphs. No meandering examples.
- One sentence per bullet where possible.
- If a section needs more than ~20 lines, it probably needs splitting or cutting.
- This rule applies to this TN too.

## Decision

Three doc types, three roles:

- **Technical Notes (TNs)** under `docs/tn/` — numbered timeline of proposals, specs, plans, explorations, research. The working surface where ideas develop.
- **Architectural Decision Records (ADRs)** under `docs/decisions/adr/` — ratified architectural decisions.
- **Convention Decision Records (CDRs)** under `docs/decisions/cdr/` — ratified conventions (style, structure, tooling).

TN stack = working surface. ADRs/CDRs = ratified output.

### TNs do NOT replace

- `docs/decisions/adr/` — ratified architectural decisions
- `docs/decisions/cdr/` — ratified convention decisions
- Crate READMEs, inline code docs
- `CLAUDE.md`

### Numbering

- Format: `TN-YYYY-NNN`, e.g. `TN-2026-001`.
- Counter resets each calendar year.
- File: `TN-YYYY-NNN-slug.md`.
- ADRs and CDRs use `ADR-NNNN` / `CDR-NNNN` zero-padded, no year reset.

### Front matter (TN)

```yaml
---
number: 2026-001
title: Short descriptive title
kind: proposal | spec | plan | exploration | research
status: draft | active | superseded
issue: "#6"                      # required
superseded-by: [TN-2026-045]    # optional
tags: [documentation, process]   # optional
---
```

- `issue` is **required** at commit time. Only `TBD` allowed on uncommitted local copies.
- `kind` — closest fit, not a hard taxonomy.
- `status` is the only field that determines authority.

### `kind` values

- `proposal` — recommends an approach
- `spec` — defines requirements or a design
- `plan` — implementation steps for a scope of work
- `exploration` — open-ended investigation, may lead nowhere
- `research` — background research on a tech, model, or pattern

### `status` values

- `draft` — work in progress, not authoritative
- `active` — agreed and current, treat as authoritative
- `superseded` — historical, follow `superseded-by`

### Numbering workflow

A TN is born from an issue. The reservation commit is the **first commit on that ticket**.

1. Open the GitHub issue.
2. Pick the next free TN number from `docs/tn/README.md`.
3. Create the file with front matter (`status: draft`, `issue: "#NNN"`) and a title heading. Body can be a single TODO line.
4. Commit + push as the first commit on the ticket. Number is now reserved.
5. Flesh out in subsequent commits on the same ticket.
6. Promote to `active` when ready.

Push races are caught by the pre-push hook — rebase, bump number, re-push.

### Supersession

1. Add `superseded-by: [TN-YYYY-NNN]` to the older TN.
2. Set the older TN's `status: superseded`.
3. Add a one-line banner at the top of the older TN body.
4. New TN references the old one for context.

### Promotion to ADR/CDR

1. Write the ADR/CDR under `docs/decisions/adr|cdr/`.
2. ADR/CDR references the TN as background.
3. TN gets `superseded-by: [ADR-XXX]` and `status: superseded`.

### Agent guidance

Read the `status` field before treating any TN as authoritative.

- `active` → authoritative
- `draft` → context only, not approved
- `superseded` → historical, follow the pointer

### Crate-scoped TNs

- May live in `crates/<name>/docs/tn/` near the code.
- Same global number sequence — numbers are unique repo-wide.
- `docs/tn/README.md` lists ALL TNs regardless of location.

## Test plan

- Every file in `docs/tn/` has `number`, `title`, `kind`, `status`, `issue`.
- `number` matches filename prefix.
- `status` ∈ {draft, active, superseded}.
- `kind` ∈ {proposal, spec, plan, exploration, research}.
- `docs/tn/README.md` lists every TN file on disk.
- `docs/decisions/README.md` lists every ADR and CDR on disk.
