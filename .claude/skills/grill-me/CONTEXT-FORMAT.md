# CONTEXT.md Format

## Structure

```md
# {Context Name}

{One or two sentence description of what this context is and why it exists.}

## Language

**Order**:
{A concise description of the term}
_Avoid_: Purchase, transaction

**Invoice**:
A request for payment sent to a customer after delivery.
_Avoid_: Bill, payment request

**Customer**:
A person or organization that places orders.
_Avoid_: Client, buyer, account

## Relationships

- An **Order** produces one or more **Invoices**
- An **Invoice** belongs to exactly one **Customer**

## Example dialogue

> **Dev:** "When a **Customer** places an **Order**, do we create the **Invoice** immediately?"
> **Domain expert:** "No — an **Invoice** is only generated once a **Fulfillment** is confirmed."

## Flagged ambiguities

- "account" was used to mean both **Customer** and **User** — resolved: these are distinct concepts.
```

## Rules

- **Be opinionated.** Pick the best term and list others as aliases to avoid.
- **Flag conflicts explicitly.** If a term is used ambiguously, call it out with a clear resolution.
- **Keep definitions tight.** One sentence max. Define what it IS, not what it does.
- **Show relationships.** Use bold term names and express cardinality where obvious.
- **Only include terms specific to this project's context.** General programming concepts don't belong.
- **Group terms under subheadings** when natural clusters emerge.
- **Write an example dialogue.** A conversation demonstrating how terms interact naturally.

## Single vs multi-context repos

**Single context (most repos):** One `CONTEXT.md` at the repo root.

**Multiple contexts:** A `CONTEXT-MAP.md` at the root lists contexts, locations, and relationships:

```md
# Context Map

## Contexts

- [Ordering](./src/ordering/CONTEXT.md) — receives and tracks customer orders
- [Billing](./src/billing/CONTEXT.md) — generates invoices and processes payments

## Relationships

- **Ordering → Billing**: Ordering emits `OrderPlaced` events; Billing consumes them
```

Infer which structure applies:

- `CONTEXT-MAP.md` exists → read it to find contexts
- Only root `CONTEXT.md` → single context
- Neither → create root `CONTEXT.md` lazily when the first term is resolved
