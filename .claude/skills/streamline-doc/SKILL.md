---
name: streamline-doc
description: Streamline a markdown doc — reorder sections for flow, kill repetition, replace prose with bullets, drop unnecessary tables, remove excessive em-dashes, sacrifice grammar for concision. Preserves meaning, never invents.
---

# streamline-doc

Make a markdown doc shorter and easier to read without losing information. Run as stage 5 of `/do-work` on every doc change before push.

## Triggers

`/streamline-doc`, "streamline this doc", "make this more concise", "tighten that up"

## Args

Optional path to a markdown file (absolute or relative to cwd). If omitted, infer from conversation context — the most recently discussed or edited markdown file. In `/do-work` mode, default to every doc touched in the current change set.

---

## Rules

- **Preserve every fact.** Streamlining is editorial, not generative. Do not invent, infer, or "improve" by adding new claims. If something seems wrong, flag it — don't silently fix.
- **Preserve code blocks, links, anchors, frontmatter.** Other docs may link to specific headings; renaming a heading breaks anchors. If renaming a heading is genuinely needed, flag it before doing it.
- **Honour repo style.** If the repo has a writing style guide (e.g. `docs/dev-practices/*`, `STYLE.md`, or a "Writing style" section in `AGENT.md`/`CLAUDE.md`), read it first and apply its rules.
- **Sacrifice grammar for concision.** Drop articles, drop "that"/"which" where meaning is clear, prefer noun phrases over full sentences in bullets.

## Streamlining moves

Apply in this order:

1. **Cut filler** — introductions, transitions ("In this section we will…"), summary paragraphs, pleasantries, hedging ("it might be worth considering"), redundant qualifiers.
2. **Merge repetition** — same point said twice in different words → keep the sharper version, delete the other. Same fact in two sections → keep where it belongs, cross-reference if needed.
3. **Prose → bullets** — any paragraph listing 3+ things becomes a bulleted list. Long paragraphs explaining a process become numbered steps.
4. **Tables → lists** — keep tables only when truly two-dimensional (genuine row × column data). A table with one meaningful column is a list. A table whose second column is just commentary is a list.
5. **Remove excessive em-dashes** — em-dashes pile up in LLM prose. Budget: **max one em-dash per paragraph outside bullet-style leaders** (e.g. `- **Thing** — description`, which is fine).
   - Apposition / definition → comma or parentheses, or two sentences.
   - Parenthetical aside → parentheses or comma.
   - Emphatic pause → rewrite cleaner without the dash.
   - Don't mass-replace blindly — rewrite each site so the prose still reads well.
6. **Sentence diet** — target <20 words per sentence. Active voice, imperative mood. "Register the extension" not "The extension should be registered".
7. **Reorder for BLUF** — bottom line up front. Most critical info at the top of the doc and the top of each section. Definitions before examples. Decisions before rationale. If a later section is the actual headline, promote it.
8. **Trim code blocks** — code in docs illustrates a concept; it is not the implementation. Strip imports, error handling, boilerplate, and unrelated fields until only the lines that demonstrate the point remain. Replace omitted regions with `// …` or `...`. Never write the code _for_ the subject — if a snippet has grown into a working implementation, that's drift; cut it back.

## Process

### Step 1 — Resolve and read

- Resolve the target path(s). Confirm they exist and are `.md`.
- Read them fully, plus any repo style guide.
- Note: word count, section structure, frontmatter, em-dash count per paragraph.

### Step 2 — Plan structural changes

Before editing, identify:

- **Section reorderings** — which sections should move and why
- **Heading renames** — if any (flag separately, anchors may break)
- **Sections to merge or split**
- **Tables to convert**

If structural changes are non-trivial, show the user the plan in 5-10 lines and wait for go-ahead. Skip this gate for purely local streamlining (no reordering, no heading changes). In autonomous `/do-work` mode, skip the gate entirely and apply the plan.

### Step 3 — Streamline

Edit the file in place. Apply the moves above. Keep frontmatter, code blocks, link targets, and heading text (unless explicitly renaming).

### Step 4 — Report

Tell the user (or coordinator in autonomous mode):

- Old word count → new word count (and % reduction)
- Em-dashes before → after
- Structural changes made (sections moved, tables converted)
- Anything flagged for their attention (suspect facts, anchor risks, content you weren't sure whether to cut)

Do not summarise the edits line-by-line — the diff is the summary.

---

## Anti-patterns

- **Don't break working code.** Trimming a snippet to its illustrative core is fine; rewriting logic isn't. If you can't tell which lines are essential, leave the block alone and flag it.
- **Don't merge sections that serve different audiences** (e.g. "Quick start" and "Reference") just because they overlap.
- **Don't delete content you don't understand.** Flag it instead.
- **Don't rewrite for style alone.** If a sentence is already tight, leave it.
- **Don't replace every em-dash.** Bullet-style leaders, and paragraphs with a single well-placed dash, are fine.
