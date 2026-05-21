# ADR Format

ADRs live in `docs/adr/` and use sequential numbering: `0001-slug.md`, `0002-slug.md`, etc.

Create the `docs/adr/` directory lazily — only when the first ADR is needed.

## Template

```md
# {Short title of the decision}

{1-3 sentences: what's the context, what did we decide, and why.}
```

That's it. An ADR can be a single paragraph. The value is in recording _that_ a decision was made and _why_.

## Optional sections

Only include when they add genuine value. Most ADRs won't need them.

- **Status** frontmatter (`proposed | accepted | deprecated | superseded by ADR-NNNN`)
- **Considered Options** — only when rejected alternatives are worth remembering
- **Consequences** — only when non-obvious downstream effects need calling out

## Numbering

Scan `docs/adr/` for the highest existing number and increment by one.

## When to offer an ADR

All three must be true:

1. **Hard to reverse** — cost of changing mind later is meaningful
2. **Surprising without context** — a future reader will wonder "why on earth?"
3. **The result of a real trade-off** — genuine alternatives existed

### What qualifies

- Architectural shape ("monorepo", "event-sourced write model")
- Integration patterns between contexts
- Technology choices that carry lock-in (not every library — just ones that would take a quarter to swap)
- Boundary and scope decisions (the explicit no-s are as valuable as the yes-s)
- Deliberate deviations from the obvious path (stops the next engineer from "fixing" something deliberate)
- Constraints not visible in code (compliance, SLAs, partner contracts)
- Rejected alternatives when the rejection is non-obvious
