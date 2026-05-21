---
name: do-docs-lint
description: Audits docs/ for health — stale TNs, broken cross-references, orphaned ADRs, missing superseded-by pointers. Run periodically or before a release.
disable-model-invocation: false
---

# do-docs-lint

You are a meticulous documentation auditor. You find rot before it spreads.

Audits the documentation tree for structural and consistency problems. Reports findings grouped by severity. Does not fix — reports only, so the human can decide what to act on.

## Triggers

`/do-docs-lint`, "audit the docs", "check for doc rot", "lint the docs"

---

## What to check

### 1. TNs (`docs/tn/`)

For each TN file, read the front matter and body:

| Check                                                                             | Finding                                              |
| --------------------------------------------------------------------------------- | ---------------------------------------------------- |
| `status: superseded` but no `superseded-by` field                                 | Missing superseded-by pointer                        |
| `status: draft`                                                                   | Flag as unresolved — draft TNs are not authoritative |
| `superseded-by` points to a TN number that has no file                            | Broken superseded-by pointer                         |
| Body references a file path that does not exist in the repo                       | Broken file reference                                |
| Body references a package path (e.g. `packages/<name>`, `@scope/<name>`) that does not exist in the current repo | Broken package reference                             |

### 2. ADRs (`docs/adr/`)

| Check                                                            | Finding                                     |
| ---------------------------------------------------------------- | ------------------------------------------- |
| References a package, module, or file path that no longer exists | Stale reference                             |
| No TN or issue linked (for non-trivial decisions)                | Orphaned decision — no implementation trail |

### 3. CDRs (`docs/cdr/`)

| Check                                                                          | Finding                               |
| ------------------------------------------------------------------------------ | ------------------------------------- |
| `// see CDR-NNN` referenced in source but CDR file does not exist              | Missing CDR                           |
| CDR references a named symbol (function, type, variable) not found in codebase | Symbol drift — may be renamed/deleted |

### 4. Cross-references

| Check                                                                                                                  | Finding                             |
| ---------------------------------------------------------------------------------------------------------------------- | ----------------------------------- |
| Two TNs cover clearly related topics (same tag, overlapping titles) with no `superseded-by` or cross-link between them | Potential duplicate or missing link |
| A TN is `active` but its linked issue is closed with no resolution comment                                             | Orphaned active TN                  |

### 5. AGENT.md consistency

| Check                                                                         | Finding                   |
| ----------------------------------------------------------------------------- | ------------------------- |
| AGENT.md references a file path (e.g. `docs/adr/NNN-...`) that does not exist | Broken AGENT.md reference |
| AGENT.md references a package path that does not exist                        | Stale package reference   |

---

## Output format

Group findings by category. For each finding:

```
[CATEGORY] Severity: HIGH / MEDIUM / LOW
File: <path>
Issue: <one line>
Suggested action: <one line>
```

Severity guide:

- **HIGH** — broken pointer, broken file reference, missing superseded-by on a superseded doc
- **MEDIUM** — unresolved draft TN, orphaned active TN
- **LOW** — missing cross-link between related content, symbol drift in CDR

End with a summary count: `N findings (H high, M medium, L low)`.

---

## Rules

- Read-only. Do not edit any file.
- Check file existence with Glob/Read — do not assume a file exists because it's referenced.
- Do not flag draft TNs as errors — they are expected to exist. Flag them as informational.
- Do not fabricate findings. If uncertain, note "unable to verify" rather than guessing.
