# bulletd — Product Requirements Document

## 1. Overview

**bulletd** is a structured bullet logging system implemented as a Rust TUI and MCP server. It adapts the Bullet Journal method for software engineers: short bullets with typed markers, daily logs, and a forced migration loop that kills low-value tasks through friction.

### 1.1 Design Principles

1. **Structured data, open format** — Markdown files on disk. No database, no protocol. Any tool that can read/write files (including an AI agent with basic file tools) can interact with the data as a fallback. Files are human-readable as-is in any Markdown viewer.
2. **Deterministic schema** — The TUI and MCP server operate on typed fields via GFM table structure. No ambiguous parsing, no formatting drift.
3. **Friction is a feature** — Migrating a task forces you to re-commit to it. Tasks that aren't worth rewriting die naturally.
4. **Single stream** — One log per day. No fragmented lists, no multi-board complexity.

### 1.2 Interfaces

| Interface | Purpose |
|-----------|---------|
| TUI (`bulletd`) | Primary daily interaction — add, review, migrate bullets |
| MCP server (`bulletd serve`) | AI agent access — deterministic CRUD without loading raw files |
| Raw Markdown files | Fallback — human or AI can read/edit directly when TUI/MCP unavailable |

---

## 2. Data Model

### 2.1 Bullet Types and Status

Every bullet is represented by a single emoji in the Status column. The emoji encodes both the type (task, event, note) and the current state:

| Emoji | Meaning | Description |
|-------|---------|-------------|
| 📌 | Open task | Something actionable — not yet acted on |
| ✅ | Done | Task completed |
| ➡️ | Migrated | Task moved to another day |
| ❌ | Cancelled | Task dropped — no longer relevant |
| 📥 | Backlogged | Task moved to backlog.md for later |
| 📅 | Event | Something that happened or is scheduled |
| 📝 | Note | Information, context, thoughts |

Tasks have state transitions: `📌 → ✅`, `📌 → ➡️`, `📌 → ❌`, `📌 → 📥`. Events and notes are immutable records — their status never changes.

### 2.2 Daily Log File

One file per day, stored at:
```
~/.local/share/bulletd/logs/YYYY-MM-DD.md
```
(or `$XDG_DATA_HOME/bulletd/logs/`)

#### File format

Each daily log is a Markdown file containing an HTML comment header (invisible in rendered view) and a GFM table:

```markdown
<!--
  ⚠️ bulletd managed file — do not hand edit
  Hand editing may break bulletd TUI or MCP server.

  Format: GFM table with columns: Status | Bullet | Notes | Migration | ID

  Status Emoji Reference:
    📌  Open task — not yet acted on
    ✅  Done — completed
    ➡️  Migrated — moved to another day
    ❌  Cancelled — dropped
    📅  Event — something that happened or is scheduled
    📝  Note — information, context, thoughts

  Notes: Optional context. Use <br> for multiple lines.

  Migration: Traceability for migrated tasks. Rendered as clickable links.
    "to YYYY-MM-DD/ID" links to the target file
    "from YYYY-MM-DD/ID" links to the source file

  ID: 8-char hex, unique within this file.
      Cross-file references use date/id format (e.g. 2026-04-05/c5a1d9e7)
-->

# 2026-04-05

| Status | Bullet | Notes | Migration | ID |
|--------|--------|-------|-----------|-----|
| ✅ | Fix Android crash on startup | | | a7f3b2c1 |
| ✅ | Review PR #142 | Waiting on CI to pass<br>Approved after second round | | e9d4c1a8 |
| 📅 | Sprint planning 10am | | | b2e8f4d3 |
| 📅 | 1:1 with Sarah | Discussed promotion timeline | | d4a9e1f2 |
| ➡️ | Investigate memory leak | Crash only on low-end Samsung devices<br>Check logcat for OOM patterns | [to 2026-04-06/d8f2a1b5](./2026-04-06.md) | c5a1d9e7 |
| ❌ | Write migration script for user table | Descoped — handled by platform team | | f1b8d3a2 |
| 📝 | New compliance requirements from legal | Affects auth middleware token storage<br>Meeting with legal next week | | f8b3e2a4 |
| ✅ | Deploy hotfix to staging | Ran smoke tests post-deploy | | b9d2f4c8 |
| 📝 | Jake out next week — cover on-call | | | e2a4b7d9 |
```

#### Rules

1. One file per day. File is named `YYYY-MM-DD.md`.
2. HTML comment header contains format documentation and emoji reference.
3. H1 heading contains the date.
4. GFM table with five columns: Status, Bullet, Notes, Migration, ID.
5. IDs are 8-character hex strings, unique within the file.
6. New bullets are appended as new rows — never reorder or insert.
7. The file is fully rewritten on every mutation (write to `.tmp`, then rename).
8. Tasks default to `📌` (open) when created.
9. Events (`📅`) and notes (`📝`) are immutable — their status never changes.
10. Notes column uses `<br>` for multiple lines within a cell.
11. Migration column contains Markdown links for traceability (e.g., `[to 2026-04-06/id](./2026-04-06.md)`).

---

## 3. Initialization

`bulletd init` — interactive setup wizard. Prompts the user for:

1. **Data directory** — Where daily logs are stored. (e.g., `~/.local/share/bulletd/logs/`)
2. **Lookback days** — How far back the open tasks view scans. (e.g., `7`)
3. **Stale threshold** — Days before a task is flagged as stale during review. (e.g., `3`)
4. **Show IDs** — Whether the TUI displays bullet IDs. (yes/no)
5. **Theme colors** — Color scheme for the TUI. Accept hex values or offer a default palette.

Creates the data directory, writes `config.toml`, and confirms setup is complete. If `config.toml` already exists, warns and asks before overwriting.

---

## 4. Core Operations

### 4.1 Add Bullet

Create a new bullet in today's log. Auto-assigns a random 8-char hex ID. If no file exists for today, create it with the HTML comment header and table structure.

### 4.2 Update Bullet

Modify a bullet's text or notes. Addressed by `(date, id)`.

### 4.3 Complete Task

Set a task's status to `✅`. Shorthand for update.

### 4.4 Cancel Task

Set a task's status to `❌`. Shorthand for update.

### 4.5 Migrate Task

Move an open task to a target date:
1. Set source bullet's status to `➡️` and add Migration link `[to YYYY-MM-DD/target_id](./YYYY-MM-DD.md)`.
2. Create a new bullet in the target day's log with status `📌`, same text, and Migration link `[from YYYY-MM-DD/source_id](./YYYY-MM-DD.md)`.
3. The new bullet gets a fresh 8-char hex ID in the target day.

### 4.6 Unmigrate Task

Reverse a migration. Addressed by `(date, id)` on the **source** bullet (the one marked `➡️`):
1. Set source bullet's status back to `📌`. Clear the Migration cell.
2. If the target bullet is **untouched** (no edits, no added notes since creation), delete it.
3. If the target bullet has been **modified** (text changed, notes added, etc.), set it to `❌` instead — preserving any work done there.
4. If the target bullet has itself been migrated onward, the unmigrate is **blocked** — you can't unwind a chain mid-flight. Cancel the leaf task manually first.

### 4.7 Backlog Task

Move an open task to the backlog:
1. Set source bullet's status to `📥` and add Migration link `[to backlog/target_id](./backlog.md)`.
2. Create a new bullet in `backlog.md` with status `📌`, same text, and Migration link `[from YYYY-MM-DD/source_id](./YYYY-MM-DD.md)`.
3. The new bullet gets a fresh 8-char hex ID in the backlog file.
4. `backlog.md` uses the same GFM table format as daily logs but with `# Backlog` as the heading.

### 4.8 Daily Review

The critical end-of-day operation. For a given date:
1. List all bullets with status `📌` (open).
2. For each, the user must choose: done (`✅`), migrated (`➡️`, to when?), backlogged (`📥`), or cancelled (`❌`).
3. After review, no tasks should remain `📌` for that day.

### 4.9 List / Query

- List bullets for a specific date
- List all open tasks across all days (the "what's hanging" view)
- List migration history for a bullet (trace its lineage across days)
- Filter by type or status

---

## 5. Migration Tracking

When a task is migrated, it forms a chain:
```
2026-04-03/a1b2c3d4 (📌) → 2026-04-04/e5f6a7b8 (📌) → 2026-04-05/c9d0e1f2 (✅)
```

This chain is reconstructable by following Migration links forward and backward. Each migrated bullet has a clickable Markdown link in the Migration column pointing to the related bullet's daily log file.

This enables the TUI to show "this task has survived 3 days" — the key friction signal.

---

## 6. Config

Stored at `~/.config/bulletd/config.toml` (or `$XDG_CONFIG_HOME/bulletd/config.toml`). Created by `bulletd init`.

```toml
[general]
# Where daily logs are stored
data_dir = "~/.local/share/bulletd/logs"
# Default number of days to look back for open tasks
lookback_days = 7

[display]
# Date format for TUI display
date_format = "%Y-%m-%d"
# Show bullet IDs in TUI
show_ids = false

[migration]
# Auto-suggest migrating open tasks older than N days
stale_threshold = 3

[theme]
# Hex color values for TUI rendering
background = "#1a1b26"
foreground = "#c0caf5"
accent = "#7aa2f7"
success = "#9ece6a"
warning = "#e0af68"
error = "#f7768e"
muted = "#565f89"
```

---

## 7. TUI Design

### 7.1 Views

| View | Description |
|------|-------------|
| **Daily Log** | Today's bullets in a scrollable list. Primary view. |
| **Review Mode** | Interactive end-of-day review — step through each open task. |
| **Open Tasks** | All open tasks across all days, grouped by date. Shows migration count. |
| **Migration History** | Trace a single task's journey across days. |

### 7.2 Key Interactions

- `a` — Add bullet (prompts for type, then text)
- `d` — Mark task done
- `x` — Cancel task
- `m` — Migrate task (prompts for target date, defaults to tomorrow)
- `u` — Unmigrate task (reverts a migrated task back to open)
- `b` — Backlog task (moves to backlog.md)
- `e` — Edit bullet text/notes
- `r` — Enter review mode for current day
- `j/k` — Navigate list
- `[/]` — Previous/next day
- `o` — Open tasks view
- `q` — Quit

### 7.3 Technology

- `ratatui` + `crossterm` for rendering
- `clap` (derive) for CLI argument parsing
- Subcommand `serve` launches the MCP server instead of the TUI

---

## 8. MCP Server Design

### 8.1 Transport

Stdio (like wdttg-tui). Launched via `bulletd serve`.

### 8.2 Tools

| Tool | Parameters | Description |
|------|-----------|-------------|
| `add_bullet` | `type`, `text`, `date?`, `notes?` | Add a bullet to a day (defaults to today) |
| `list_bullets` | `date?`, `type?`, `status?` | List bullets with optional filters |
| `update_bullet` | `date`, `id`, `text?`, `notes?` | Update bullet fields |
| `complete_task` | `date`, `id` | Mark a task as done |
| `cancel_task` | `date`, `id` | Mark a task as cancelled |
| `migrate_task` | `date`, `id`, `target_date?` | Migrate a task (defaults to tomorrow) |
| `unmigrate_task` | `date`, `id` | Reverse a migration — reverts source to open, cleans up target |
| `backlog_task` | `date`, `id` | Move a task to backlog.md |
| `list_open_tasks` | `lookback_days?` | List all open tasks across recent days |
| `daily_review` | `date?` | Get open tasks needing review for a date |
| `migration_history` | `date`, `id` | Trace a task's migration chain |

### 8.3 Why MCP over raw file access

The MCP server provides:
1. **ID assignment** — No risk of duplicate IDs
2. **Atomic writes** — Proper temp-file-then-rename
3. **Validation** — Status transitions, type enforcement
4. **Migration bookkeeping** — Creates entries in both source and target days

The AI agent uses MCP tools as the primary interface. If the MCP server is unavailable, the agent can fall back to reading/writing the Markdown files directly — the format is simple and documented.

---

## 9. Crate Responsibilities

### 9.1 bulletd-core

- Markdown/GFM table parsing and serialization (daily log files)
- Bullet CRUD operations
- Migration logic (cross-day mutations)
- Validation (status transitions, ID assignment)
- Config loading
- Query/filter operations
- File I/O (atomic writes)
- **No async. No UI. Pure business logic.**

### 9.2 bulletd-tui

- Terminal UI rendering (ratatui + crossterm)
- User input handling
- CLI argument parsing (clap)
- Subcommand routing (`bulletd` for TUI, `bulletd serve` for MCP)
- Depends on bulletd-core

### 9.3 bulletd-mcp

- MCP protocol implementation (stdio transport)
- Tool definitions and handlers
- Maps MCP tool calls to bulletd-core operations
- Depends on bulletd-core

---

## 10. File System Layout

```
~/.local/share/bulletd/
  logs/
    2026-04-03.md
    2026-04-04.md
    2026-04-05.md
    backlog.md
~/.config/bulletd/
  config.toml
```

---

## 11. Non-Goals (v1)

- No cloud sync — this is local-first
- No multi-user support
- No recurring/repeating tasks — add them manually (that's the point)
- No long-form notes — bullets are short; link out if needed
- No tags or categories — keep it flat and simple

## 12. Future Considerations

- **Backlog promotion** — TUI feature to browse `backlog.md` and promote a task back into today's daily log.
