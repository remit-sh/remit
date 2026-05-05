# Configuration reference

## Remit home

Everything Remit reads and writes lives under a single root: the *Remit home*.

Resolution order:

1. `$REMIT_HOME` — used directly if set and non-empty.
2. `$HOME/.remit` — fallback.

Empty strings count as unset. Both unset/empty → fail closed: deny, exit 2, stderr `remit: <ctx>: neither REMIT_HOME nor HOME environment variable is set; deny`.

## On-disk layout

```
<remit_home>/
├── config.toml          # see schema below
├── scopes/              # bound agent scopes — one file per agent id
├── pending-scopes/      # staged scopes for not-yet-started children
└── remit.log            # append-only audit log
```

Subdirectories are created on first use.

## `config.toml`

Loaded from `<remit_home>/config.toml`. Missing file → defaults. Malformed TOML or unknown fields → error (the hook fails closed).

| Field | Type | Default | Description |
|---|---|---|---|
| `dev_bypass` | `bool` | `false` | When `true`, the classifier still runs but `Verdict::Red` does not block. The would-have-denied **Reason** is logged to stderr as `[dev-bypass] would have denied: ...`. |
