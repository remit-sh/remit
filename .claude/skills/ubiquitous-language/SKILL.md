# Domain Context

Extract and formalize domain terminology from the current conversation into the project's CONTEXT.md files.

## Process

1. **Read the context map** — check `CONTEXT-MAP.md` at root to understand existing contexts
2. **Read the relevant CONTEXT.md** — identify which context the current conversation relates to
3. **Scan the conversation** for domain-relevant nouns, verbs, and concepts
4. **Identify problems**:
   - Same word used for different concepts (ambiguity)
   - Different words used for the same concept (synonyms)
   - Vague or overloaded terms
5. **Propose updates** — new terms, refined definitions, or reclassification
6. **Update the relevant CONTEXT.md** in place
7. **Output a summary** inline in the conversation

## Which CONTEXT.md to update

- Look at existing `CONTEXT.md` files in the repo (root and per-package) — `CONTEXT-MAP.md` indexes them
- Place the term in the most specific context that owns it
- Cross-cutting terms go in the root `CONTEXT.md`
- If a new domain emerges that doesn't fit existing contexts → create a new `CONTEXT.md` and add it to `CONTEXT-MAP.md`

## Output Format

Each CONTEXT.md uses this structure:

```md
# {Context Name}

{One or two sentence description of what this context is and why it exists.}

## Language

**Term**:
{A concise definition — one sentence max. What it IS, not what it does.}
_Avoid_: Synonym1, synonym2

## Relationships

- A **Term** produces one or more **OtherTerms**

## Example dialogue

> **Dev:** "Question using the terms?"
> **Domain expert:** "Answer demonstrating precise usage."

## Flagged ambiguities

- "word" was used to mean both **X** and **Y** — resolved: these are distinct concepts.
```

## Rules

- **Be opinionated.** Pick the best term and list others as aliases to avoid.
- **Flag conflicts explicitly.** Call out ambiguity with a clear resolution.
- **Keep definitions tight.** One sentence max. Define what it IS.
- **Show relationships.** Bold term names, express cardinality where obvious.
- **Only domain terms.** Skip generic programming concepts unless they have domain-specific meaning.
- **Group by context.** Terms belong in the context where they're defined. A term used across contexts should live in the context that owns it and be referenced from others via the context map.
- **Write an example dialogue.** 3-5 exchanges demonstrating precise usage and clarifying boundaries.

## Re-running

When invoked again:

1. Read `CONTEXT-MAP.md` and all referenced CONTEXT.md files
2. Identify which context the new terms belong to
3. Incorporate new terms, update definitions if understanding has evolved
4. Re-flag any new ambiguities
5. Update example dialogues to incorporate new terms
