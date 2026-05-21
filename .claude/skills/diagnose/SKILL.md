---
name: diagnose
description: Disciplined diagnosis loop for hard bugs and performance regressions. Reproduce → minimise → hypothesise → instrument → fix → regression-test. Use when user says "diagnose this", "debug this", reports a bug, says something is broken/throwing/failing, or describes a performance regression.
---

# Diagnose

A discipline for hard bugs. Skip phases only when explicitly justified.

## Phase 1 — Build a feedback loop

**This is the skill.** Everything else is mechanical. If you have a fast, deterministic, agent-runnable pass/fail signal for the bug, you will find the cause. If you don't, no amount of staring at code will save you.

Spend disproportionate effort here. Be aggressive. Be creative. Refuse to give up.

### Strategies — try roughly in this order

1. **Failing test** at whatever seam reaches the bug — unit, integration, e2e.
2. **Curl / HTTP script** against a running dev server.
3. **CLI invocation** with a fixture input, diffing stdout against a known-good snapshot.
4. **Headless browser script** (Playwright MCP) — drives the UI, asserts on DOM/console/network.
5. **Replay a captured trace.** Save a real network request / payload / event log to disk; replay it through the code path in isolation.
6. **Throwaway harness.** Spin up a minimal subset of the system that exercises the bug code path with a single function call.
7. **Property / fuzz loop.** If the bug is "sometimes wrong output", run 1000 random inputs and look for the failure mode.
8. **Bisection harness.** If the bug appeared between two known states, automate `git bisect run`.
9. **Differential loop.** Run the same input through old-version vs new-version and diff outputs.
10. **HITL script.** Last resort — if a human must interact, write a script that drives them through a structured loop and captures output.

If none of these work, stop and explain why to the user. Do not proceed without a feedback loop.

## Phase 2 — Minimise the reproduction

Shrink the input, strip unrelated config, reduce the system to the smallest thing that still fails. The smaller the reproduction, the more constrained the hypothesis space.

## Phase 3 — Hypothesise

Form at most 3 candidate root causes. For each, state:

- What you'd expect to observe if it's correct
- How to falsify it

Rank by likelihood. Do not implement a fix based on hypothesis alone.

## Phase 4 — Instrument and verify

Add the minimal instrumentation to confirm or eliminate each hypothesis. This might be:

- A log line at a specific point
- A breakpoint condition
- A counter or timer
- A temporary assertion

Run the feedback loop. If all hypotheses are eliminated, return to Phase 3 with new information.

## Phase 5 — Fix

Implement the minimal fix. Run the feedback loop — it must pass. If the fix is non-obvious, explain why it works.

## Phase 6 — Regression test

Convert the feedback loop from Phase 1 into a permanent test (if one doesn't already exist). The test should:

- Fail without the fix applied
- Pass with it
- Describe the bug scenario in its name, not the implementation detail

## Rules

- Never guess-fix. No "try this and see if it helps" without a hypothesis.
- Never skip Phase 1. A fix without a feedback loop is a coin flip.
- Never add instrumentation that stays in production. Remove it after Phase 4.
- If the bug is in a dependency you don't control, document the workaround and file upstream if appropriate.
