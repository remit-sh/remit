# Decision Records

Architectural Decision Records (ADRs) and Convention Decision Records (CDRs) documenting the design and conventions of remit.

Working ideas live in [`docs/tn/`](../tn/) as Technical Notes. Once ratified, they get promoted here. Layout, numbering, front matter, and review-agent compliance rules are in [CDR-0004](cdr/CDR-0004-doc-system-layout.md).

## Architectural Decision Records (ADR)

| ID | Decision | Status |
|---|---|---|
| [ADR-0001](adr/ADR-0001-hexagonal-architecture.md) | Strict hexagonal architecture with domain-owned ports | active |
| [ADR-0002](adr/ADR-0002-fail-closed-security-posture.md) | Fail-closed security posture | active |
| [ADR-0003](adr/ADR-0003-single-composition-root-per-binary.md) | Single composition root per adapter binary | active |
| [ADR-0004](adr/ADR-0004-newtype-boundary-validation.md) | Boundary validation via newtypes | active |

## Convention Decision Records (CDR)

| ID | Convention | Status |
|---|---|---|
| [CDR-0001](cdr/CDR-0001-ubiquitous-language-contract.md) | Ubiquitous language is the vocabulary contract | active |
| [CDR-0002](cdr/CDR-0002-tests-in-separate-files.md) | Tests live in separate files | active |
| [CDR-0003](cdr/CDR-0003-test-coverage-policy.md) | Near-100% coverage with port contract tests | active |
| [CDR-0004](cdr/CDR-0004-doc-system-layout.md) | Documentation system — TN, ADR, CDR | active |
| [CDR-0005](cdr/CDR-0005-code-style-current-state-only.md) | Code style — current intended state only | active |
