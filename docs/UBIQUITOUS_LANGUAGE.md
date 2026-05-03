# Ubiquitous Language

## Product

| Term | Definition | Aliases to avoid |
| --- | --- | --- |
| **Remit** | The product name. | — |
| **Scope** | The agent's task — what it's trying to accomplish (e.g. "develop the front-end history component"). Not an allowlist. | Remit, intent, permissions |
| **Semantic firewall** | Relevance check between **Scope** and **Action**. | Guardrail, policy engine |

## Sessions

| Term | Definition | Aliases to avoid |
| --- | --- | --- |
| **Session** | One run of an agent against one **Scope**. May be top-level or a **Subagent**. | Conversation, run |
| **Subagent** | A child agent acting under a parent's **Scope**. Runtime-agnostic — some runtimes realise subagents in-process, some as peer **Sessions**; the domain treats them uniformly. | Spawn, team member, child session |

## Classification

| Term | Definition | Aliases to avoid |
| --- | --- | --- |
| **Action** | A tool call to be classified. | Tool call, command |
| **Classifier** | Turns a **Classification input** into a **Verdict**. | Engine, judge |
| **Classification input** | **Session** + **Action** + working dir. | Request |
| **Verdict** | **Green** \| **Red**. | Decision, ruling |
| **Green** / **Red** | Action serves / does not serve the **Scope**. | Pass / Deny |
| **Reason** | Justification on a Red **Verdict**. | Explanation |

## Documentation

| Term | Definition | Aliases to avoid |
| --- | --- | --- |
| **TN** | Technical Note — numbered timeline doc (proposal, spec, plan, exploration, research). | Design doc, RFC |
| **ADR** | Architectural Decision Record — ratified architectural decision. | — |
| **CDR** | Convention Decision Record — ratified style/tooling/layout convention. | — |

## Relationships

- A **Session** has exactly one **Scope**.
- A **Subagent** runs under a parent agent and inherits the parent's **Scope**.
- The **Classifier** maps one **Classification input** to one **Verdict**.
- Only Red **Verdicts** carry a **Reason**.

## Example dialogue

> **Dev:** "**Scope** is 'develop the front-end history component'. Agent reads `~/.ssh/id_rsa` — **Verdict**?"
> **Expert:** "Red. No relationship to the task. The **Reason** spells out the mismatch."
> **Dev:** "And `npm install lodash`?"
> **Expert:** "Green. Plausible for a front-end task. **Classifier** picks a side; we don't do soft warnings in v1."
> **Dev:** "So **Scope** is relevance, not permission?"
> **Expert:** "Exactly. Not an allowlist."

## Flagged ambiguities

- **Remit vs Scope** — Remit is only the product name; the task is **Scope**.
- **Claude Code hook vs git hook** — always qualify; never bare "hook".
- **Code drift** — rename `Spawn`/`SpawnId` → `Subagent`/`SubagentId`; rename `Decision` enum → `Verdict` (stays binary Green/Red).
