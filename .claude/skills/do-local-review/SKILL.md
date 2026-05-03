---
name: do-local-review
description: Risk-scaled local review before pushing to main. Trivial changes get self-review, standard changes get one agent, security/architecture changes get full parallel review. Findings applied inline. Summary posted as issue comment.
disable-model-invocation: false
---

# do-local-review

Local review scaled to the risk of the change. Called by `do-push` — can also be run standalone. Findings are applied before any push happens. Output is an issue comment, not PR review comments.

## Triggers
`/do-local-review`, "review my changes", "check the architecture", "local review"

---

## Stage 1 — Read the diff

```bash
git diff main...HEAD --stat
git diff main...HEAD
```

Also read any relevant context: changed files in full, related tests, impacted interfaces.

Classify risk (confirm with caller if invoked from do-push, otherwise state it):

| Level | Criteria |
|---|---|
| **Trivial** | Docs, comments, config, text, test data, annotations |
| **Standard** | Single domain, clear scope, no security/auth/crypto/ports touched |
| **Security/Architecture** | Ports, crypto, auth, wire format, new deps, cross-cutting changes, CLAUDE.md security invariants touched |

---

## Stage 2 — Review (scaled)

### Trivial

Self-review only. Check:
- Nothing accidentally committed (debug statements, temp files, hardcoded values)
- Change matches the stated intent
- No obvious logic errors

Output: one-paragraph summary. No agent spawn.

### Standard

Spawn one `architect` agent against the diff. Brief:
> "Review this diff for architectural quality: naming consistency, separation of concerns, boundary violations, duplication. This is a trunk-based repo — findings should be actionable and proportionate. No PR comments — return a structured list of findings."

Apply all HIGH findings before proceeding. Flag MEDIUM findings in the issue comment. LOW findings are informational.

### Security/Architecture

Spawn two agents in parallel:
1. `architect` — as above
2. `security-analyst` — brief: "Review this diff for security issues: OWASP top 10, input validation, secret handling, auth/crypto correctness, dependency risk. Return findings with severity."

**ADR escalation:** if either agent flags a finding that touches the security model invariants (fail-closed, no hook bypass, cross-session access, HMAC integrity, claim token replay, session ID validation) → hard stop. Do not proceed. Raise with user: "This change touches a security invariant and requires an ADR before it can land."

Apply all HIGH/CRITICAL findings before proceeding. Do not push with unresolved HIGH security findings.

---

## Stage 3 — Produce review summary

Format for the issue comment:

```markdown
## Review summary

**Risk level:** [Trivial / Standard / Security/Architecture]
**Outcome:** [Clean / Findings applied / Findings flagged]

### Findings applied
- [list of changes made based on review, or "none"]

### Findings flagged (not blocking)
- [list of medium/low findings for awareness, or "none"]

### Reviewer notes
[Any architectural observations worth recording for future context]
```

Return this to the caller (`do-push`) to post as the issue comment. If run standalone, post it directly.

---

## Rules

- Findings are applied inline before push — review is not a paper exercise
- HIGH findings always block push until resolved
- ADR escalation is a hard stop — no exceptions, no "we'll fix it in a follow-up"
- Review summary always gets posted to the issue — even if trivial ("self-reviewed, no findings")
- Never post line-by-line PR comments — there is no PR. Output goes to the issue.
