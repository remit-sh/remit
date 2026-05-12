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
├── config.toml             # see schema below
├── agents/                 # all agent-keyed state (one subdir per agent id)
│   └── <agent-id>/
│       ├── scope.json      # this agent's bound scope
│       ├── pending.json    # staged scope for the next child this agent spawns
│       ├── disabled.marker # disable flag (root agents only; existence = disabled)
│       └── prompt.lock     # per-root flock used during the verdict prompt
└── remit.log               # append-only audit log
```

`agents/` and per-agent subdirs are created lazily on first write.

## `config.toml`

Loaded from `<remit_home>/config.toml`. Missing file → defaults. Malformed TOML or unknown fields → error (the hook fails closed).

| Field | Type | Default | Description |
|---|---|---|---|
| `dev_bypass` | `bool` | `false` | When `true`, the classifier still runs but `Verdict::Red` does not block. The would-have-denied **Reason** is logged to stderr as `[dev-bypass] would have denied: ...`. |
