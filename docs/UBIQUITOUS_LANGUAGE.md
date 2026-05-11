# Ubiquitous Language

## Product

| Term | Definition | Aliases to avoid |
| --- | --- | --- |
| **Remit** | The product name. | — |
| **Scope** | The agent's task — what it's trying to accomplish (e.g. "develop the front-end history component"). Not an allowlist. | Remit, task, intent, permissions |
| **Semantic firewall** | Relevance check between **Scope** and **Action**. | Guardrail, policy engine |

## Agents

| Term | Definition | Aliases to avoid |
| --- | --- | --- |
| **Agent** | Domain umbrella for any scope-bearing runtime instance — top-level or **Subagent**, in-process or peer process. Cross-runtime: a Claude Code session, a Claude Code in-process subagent, a Cursor agent, a Codex task, etc., are all **Agents**. | Session (runtime-specific), worker, instance |
| **Subagent** | A child **Agent** with its own **Scope**, set by the parent at spawn time (extracted from the spawn-tool payload). Runtime-agnostic. | Spawn, team member, child session |
| **Session** | A Claude-Code-specific runtime term: one forked `claude` process. Use only when discussing Claude Code internals. The domain term is **Agent**. | — |
| **Bundled scope** | The **Scope** carried in a parent's spawn-tool payload (e.g. `Agent.prompt`) that the hook lifts as the child's scope at spawn time. | Inherited scope (no longer used) |
| **Agent type** | Runtime-supplied label for an **Agent**'s role (e.g. `architect`, `test-writer`, `general-purpose`). Carried in hook payloads for typed Agents (in-process **Subagents** and peer-process team Subagents); absent for root Agents. Selection key for **Agent template**. | role, kind, agent kind |

## Classification

| Term | Definition | Aliases to avoid |
| --- | --- | --- |
| **Action** | A tool call to be classified. | Tool call, command |
| **Classifier** | Turns a **Classification input** into a **Verdict**. | Engine, judge |
| **Classification input** | **Agent** + **Action** + working dir. | Request |
| **Verdict** | **Green** \| **Red**. | Decision, ruling |
| **Green** / **Red** | Action serves / does not serve the **Scope**. | Pass / Deny |
| **Reason** | Justification on a Red **Verdict**. | Explanation |
| **Standing guidance** | Umbrella for trusted policy text fed to the **Classifier** every call — currently **Project guidance** and **Agent template**; reserved slots for user-level guidance. | Project context, policy |
| **Project guidance** | Contents of `REMIT.md`, the project-scoped slot of **Standing guidance**. Peer to the firewall framing; not an allowlist. | Standing policy, project context, project policy |
| **Agent template** | Per-**Agent type** slot of **Standing guidance**. Stored at `.remit/agent-templates/<agent_type>.md`. Policy specific to this agent type, sitting alongside **Project guidance**; commonly orthogonal (e.g. a `research-web` template authorising network fetches that the project doesn't mention). | Role template, subagent template |

## Documentation

| Term | Definition | Aliases to avoid |
| --- | --- | --- |
| **TN** | Technical Note — numbered timeline doc (proposal, spec, plan, exploration, research). | Design doc, RFC |
| **ADR** | Architectural Decision Record — ratified architectural decision. | — |
| **CDR** | Convention Decision Record — ratified style/tooling/layout convention. | — |

## Relationships

- An **Agent** has exactly one **Scope**.
- A **Subagent** has its own **Scope**, set by the parent at spawn time. No inheritance.
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
