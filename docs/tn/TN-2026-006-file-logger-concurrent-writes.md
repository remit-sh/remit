---
number: 2026-006
title: FileLogger atomic appends under concurrent processes
kind: plan
status: active
issue: "#24"
tags: [file-logger, concurrency, observability, fail-closed]
---

# TN-2026-006: FileLogger atomic appends under concurrent processes

> One whole, well-formed line per `log_line` call, even with many `claude-code-hook` processes appending concurrently.

## Problem

- `claude-code-hook` runs once per Claude Code tool invocation — fresh OS process each time.
- Under parallel subagent activity, many such processes append to the shared log simultaneously.
- `log_line` uses `writeln!` on an unbuffered `File` opened `append(true)`. `writeln!` lowers to multiple `write` syscalls (format pieces, then `"\n"`).
- `O_APPEND` is only guaranteed atomic for writes ≤ `PIPE_BUF` (~4 KB). Real entries (Write tool args, Agent spawn payloads with scope text) routinely exceed that.
- Observed in a 6-subagent dev session: interleaved fragments, `hook-event: PreToolUsehook-event: PreToolUse`, missing newlines.

## Impact

- Audit trail corruption in a security tool.
- Future log consumers (CLI #19, SQLite ingester) break on malformed lines.

## Fix

Two changes in `FileLogger::log_line`:

1. **One buffer, one syscall.** Pre-build `prefix + line + "\n"` into a `String`, call `file.write_all(buf.as_bytes())`. Removes the multi-syscall split `writeln!` introduces.
2. **Exclusive advisory lock around the write.** `file.lock()` (stable in std since 1.89) before `write_all`, drop the file after to release. Serialises writes across all processes holding the same inode open, regardless of payload size.

Cross-process correctness independent of payload size, no new deps.

Per-call cost: one extra syscall. Acceptable for a hook that already pays an `open`.

## Why not in-process `Mutex`

`FileLogger` is `Clone` and runs in single-shot processes. Different processes get different `Mutex` instances, so an in-process mutex provides no guarantee. Cross-process serialisation requires the kernel — `flock` / `File::lock`.

## Test strategy

TDD. Reproduction first, fix second.

Reproduction (failing) test in `tests/integration.rs`:

- N threads (≥ 16), each calling `log_line` M times (≥ 50), payload ≥ 8 KB body, distinct marker per thread.
- Total line count == N × M.
- Every line matches its expected shape: starts with the thread's marker, no marker substring twice in one line — directly the `PreToolUsehook-event: PreToolUse` failure mode.

Existing tests stay green: line format, marker prefixing, append semantics.

In-process threads are the cheapest reproduction. Cross-process behaviour follows from `flock` semantics; covered by the lock contract, not a separate test.

## Out of scope

- Switching log format to NDJSON (docstring says NDJSON, format is plain text — pre-existing drift).
- Buffered writes / batching. Per-line atomicity is the contract.
- Log rotation, size caps.

## Files touched

- `crates/file-logger/src/lib.rs`: rewrite `log_line` body — one buffer + `File::lock` + `write_all`.
- `crates/file-logger/tests/integration.rs`: concurrent-writers reproduction test.
- `docs/tn/README.md`: index entry.
