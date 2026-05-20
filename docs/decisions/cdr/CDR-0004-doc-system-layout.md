---
number: 0004
title: Documentation system — TN, ADR, CDR
status: active
issue: "#16"
sources: ["TN-2026-001"]
supersedes: ["TN-2026-001"]
tags: [convention, documentation, meta]
---

# CDR-0004: Documentation system — TN, ADR, CDR

> Promotes [TN-2026-001](../../tn/TN-2026-001-technical-notes-and-doc-structure.md). The TN is now superseded; this CDR is authoritative.

## Context

- The project needs a working surface for ideas in motion (TNs) and a settled surface for ratified decisions (ADRs/CDRs).
- TN-2026-001 introduced the system. Now durable; promote.

## Decision

Three doc types, three roles:

- **Technical Notes (TNs)** — `docs/tn/`. Numbered timeline of proposals, specs, plans, explorations, research. Working surface where ideas develop.
- **Architectural Decision Records (ADRs)** — `docs/decisions/adr/`. Ratified architectural decisions.
- **Convention Decision Records (CDRs)** — `docs/decisions/cdr/`. Ratified conventions (style, structure, tooling, workflow).

TN stack = working surface. ADRs/CDRs = ratified output.

### Numbering

- TNs: `TN-YYYY-NNN`, counter resets each calendar year. Filename: `TN-YYYY-NNN-slug.md`.
- ADRs: `ADR-NNNN`, zero-padded, **no year reset**. Filename: `ADR-NNNN-slug.md`.
- CDRs: `CDR-NNNN`, zero-padded, **no year reset**. Filename: `CDR-NNNN-slug.md`.

### Front matter

TN:

```yaml
---
number: 2026-001
title: Short descriptive title
kind: proposal | spec | plan | exploration | research
status: draft | active | superseded
issue: "#6"                      # required
superseded-by: [ADR-0007]        # optional
tags: [documentation, process]   # optional
---
```

ADR / CDR:

```yaml
---
number: 0001
title: Short descriptive title
status: active | superseded
issue: "#16"                     # required — the issue that ratified the decision
sources: [TN-2026-001, "CLAUDE.md#section"]   # optional citations
supersedes: [TN-2026-001]        # optional
superseded-by: [ADR-0042]        # optional
tags: [...]                      # optional
---
```

- `issue` required at commit time. `TBD` allowed only on uncommitted local copies.
- `status` is the **only** field that determines authority:
  - `draft` — work in progress, not authoritative.
  - `active` — agreed and current, treat as authoritative.
  - `superseded` — historical, follow `superseded-by`.

### Style

Same rule for TNs, ADRs, CDRs:

- Compact. Bulleted. Get to the point.
- No long prose paragraphs. No meandering examples.
- One sentence per bullet where possible.
- Section over ~20 lines → split or cut.

### Content

ADRs and CDRs carry a `## Compliance (review agents)` section listing checks review agents apply against this decision.

### Promotion (TN → ADR/CDR)

A TN graduates when its decision is durable.

1. Write the ADR/CDR under `docs/decisions/adr|cdr/`. Reference the source TN in `sources` and `supersedes`.
2. Set the original TN's `status: superseded` and `superseded-by: [<new-id>]`.
3. Add a banner at the top of the TN body: `> Superseded by [<new-id>](...)`.

### Edits to active TNs

Active TNs are the working surface — editing them in place is the norm, not an exception. Codebase-wide refactors (renames, restructures) routinely invalidate identifiers quoted in active TNs; update those in place rather than spawning supersession churn.

When a substantive edit lands (anything beyond fixing typos / dead links), append a one-line note to a trailing `## Edits` section at the bottom of the TN:

```markdown
## Edits

- 2026-05-20: rename pass — `FileLogger` → `FileClassificationHistoryWriter` etc.
- 2026-04-12: clarified the descendant-walk algorithm.
```

- Dated. One bullet per edit. No supersession metadata — the TN's `status` stays `active`.
- Trivial fixes (typo, broken link, formatting) don't need a note.
- This applies only to TNs. ADRs/CDRs change via supersession.

### Supersession (ADR ↔ ADR or CDR ↔ CDR)

1. Newer record gets `supersedes: [<old-id>]`.
2. Older record gets `status: superseded` and `superseded-by: [<new-id>]`.
3. Older record stays in place; readers find files by ID and need the pointer in front matter (git history alone isn't enough).

### Indexes

- `docs/decisions/README.md` lists every ADR and CDR on disk, with status.
- `docs/tn/README.md` lists every TN on disk regardless of location (including crate-scoped TNs under `crates/<name>/docs/tn/`).
- Index updates land in the same change as the record they describe.

### Crate-scoped TNs

- May live in `crates/<name>/docs/tn/` near the code.
- Same global TN number sequence; numbers are unique repo-wide.

### Agent guidance

- Read `status` before treating any record as authoritative.
- `active` ADRs and CDRs are authoritative. Review agents check changes against them and either flag a non-compliance or draft a new ADR/CDR for human approval.
- `draft` TNs are context only; don't enforce.

## Consequences

- Two-tier system: TNs are cheap to write; ADRs/CDRs are expensive to change.
- Review agents have an explicit, growing rulebook.
- Doc churn is bounded: settled decisions land in ADRs/CDRs and don't get rewritten in every new TN.

## Compliance (review agents)

- A new file under `docs/tn/` without TN front matter, or with an invalid `kind`/`status`, is a flag.
- A new file under `docs/decisions/{adr,cdr}/` without ADR/CDR front matter or `## Compliance` section is a flag.
- A change to an `active` ADR/CDR's `## Decision` section without a status flip to `superseded` (or clearly editorial fix) is a flag; substantive changes happen via supersession, not in-place edits.
- A substantive edit to an `active` TN without an entry in a trailing `## Edits` section is a flag. Trivial fixes (typos, link rot, formatting) don't need a note.
- `docs/decisions/README.md` and `docs/tn/README.md` must list every record on disk; missing entries are a flag.
- A TN promoted to ADR/CDR without `status: superseded` and a `superseded-by` pointer is a flag.
