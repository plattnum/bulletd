# bulletd

A highly opinionated digital bullet journal for the terminal.

bulletd was inspired by and attempts to implement a subset of the [Bullet Journal](https://bulletjournal.com/blogs/faq/what-is-the-bullet-journal-method) system — rapid logging, migration, and review — without the overhead of a full task management system. Everything is a bullet. There's no distinction between tasks, events, and notes — just bullets with a status. Each bullet can carry multi-line context notes — added at creation or edited later — so you can capture the "why" alongside the "what" if needed.

All data is stored as plain Markdown — one file per day, one GFM table per file. No database, no proprietary format. The files are readable in any text editor, any Markdown viewer, or on GitHub. The TUI and MCP server are just faster ways to work with them.

You open it in the morning, jot down what's on your plate, and throughout the day you capture what happens. At the end of the day (or whenever), you glance at what's still open and decide: done, move to tomorrow, drop it, or shelve it.

It's not Jira. It's not a project management tool. There's no priorities, no assignments, no due dates, no labels.

### Why does this exist?

I've kept a paper bullet journal for years to deal with my day to day work tasks. I always wanted to just talk to it — say "add a bullet", "what's still open", "migrate that to tomorrow" — without the friction of pen and paper. But every digital tool I tried missed the point. TODO apps come with too much of the kitchen sink: priorities, labels, due dates, recurring tasks, and endless grooming. You spend more time managing the system than using it. MCP servers that wrap SaaS tools (Linear, Todoist, etc.) are slow as molasses — every call is a round trip to someone else's API.

The idea clicked when I saw the Task Tools in Claude's Cowork. An AI managing your daily log alongside you — that's what I wanted. I tried building it as a Claude Code plugin backed by Markdown files, but it was never fully deterministic. The AI has to read files into context, parse the format, and try to enforce schema rules with no guarantees. It mostly works, but it's fragile.

So bulletd fills that gap for me: a local MCP server that owns the data format. Instead of reading and rewriting Markdown files, the AI calls typed tools — `add_bullet`, `complete_bullet`, `migrate_bullet` — and the server handles parsing, validation, and persistence. The AI doesn't need to think about file formats or worry about corrupting your data. It just translates what you say into API calls. Fast, local, deterministic, and correct by construction.

Honestly, this probably has a shelf life. The moment AI agents can generate their own schemas and programmatic access on the fly — which Cowork and similar tools are inching toward — a hand-built MCP server for a bullet journal becomes redundant. But until that day, this works and it's fast.

## Install

Requires [Rust](https://rustup.rs/) 1.85+ (2024 edition).

```bash
cargo install --path crates/bulletd-tui
```

## Setup

```bash
bulletd init
```

This creates a config file and data directory. You'll be prompted for where to store your daily logs.

## Usage

```bash
bulletd        # Open the TUI
bulletd serve  # Start the MCP server (for AI agent access)
```

### Daily Log

The main view is your daily log. Each bullet has a status:

| Status | Meaning |
|--------|---------|
| Open | Not yet acted on |
| Done | Completed |
| Migrated | Moved to another day |
| Cancelled | Dropped |
| Backlogged | Shelved for later |

### Grouped View

Press `g` to toggle grouped-by-status view. Bullets are partitioned into sections — Open, Done, Migrated, Cancelled, Backlogged — with headers showing the count for each group. Empty groups are hidden. The MCP API supports this too: pass `group_by: "status"` to `list_bullets` and the AI gets pre-grouped results without having to sort them itself.

### Keybindings

| Key | Action |
|-----|--------|
| `a` | Add a new bullet |
| `e` | Edit the selected bullet |
| `d` | Mark done |
| `o` | Reopen (set back to open) |
| `x` | Cancel |
| `m` | Migrate to tomorrow |
| `u` | Unmigrate |
| `b` | Move to backlog |
| `D` | Delete |
| `Enter` | Grab and reorder |
| `j`/`k` | Navigate up/down |
| `h`/`l` | Previous/next day |
| `r` | Review open tasks for this day |
| `O` | View all open tasks across recent days |
| `H` | Migration history for selected bullet |
| `g` | Toggle grouped-by-status view |
| `i` | Toggle icon style (minimal/emoji) |
| `q` | Quit |

### Data Format

Daily logs are stored as plain Markdown files — one per day, using GFM tables. They're readable in any text editor, any Markdown viewer, or on GitHub. No database, no proprietary format.

```
# 2026-04-06

| Status | Bullet | Notes | Migration | ID |
|--------|--------|-------|-----------|-----|
| 📌 | Fix flaky auth test | Relies on wall clock timing | | b7 |
| ✅ | Deploy to staging | | | f4 |
| ➡️ | Update SDK | Blocked on v2.1 | [to 2026-04-07/m4](./2026-04-07.md) | a9 |
```

### MCP Server

bulletd includes an MCP server so AI agents can read and write your journal programmatically:

```bash
bulletd serve
```

This exposes tools for adding bullets, changing status, migrating tasks, and querying open items — all over stdio transport.

#### Claude Code

Add to your project's `.mcp.json` (or `~/.claude/mcp.json` for global access):

```json
{
  "mcpServers": {
    "bulletd": {
      "command": "bulletd",
      "args": ["serve"]
    }
  }
}
```

If running from source instead of an installed binary:

```json
{
  "mcpServers": {
    "bulletd": {
      "command": "cargo",
      "args": ["run", "-p", "bulletd-tui", "--", "serve"],
      "cwd": "/path/to/bulletd"
    }
  }
}
```

#### Claude Desktop

Add to `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "bulletd": {
      "command": "bulletd",
      "args": ["serve"]
    }
  }
}
```

Restart Claude Desktop after saving.

#### Available MCP Tools

| Tool | Description |
|------|-------------|
| **Bullets** | |
| `add_bullet` | Add a bullet to a day's log |
| `list_bullets` | List bullets for a date, filter by status or group_by="status" |
| `list_open_bullets` | List all open bullets across recent days |
| `update_bullet` | Update a bullet's text |
| **Status** | |
| `complete_bullet` | Mark a bullet as done |
| `cancel_bullet` | Mark a bullet as cancelled |
| `open_bullet` | Set a bullet back to open |
| `batch_set_status` | Change status of multiple bullets at once |
| **Migration** | |
| `migrate_bullet` | Migrate a bullet to another day (defaults to tomorrow) |
| `unmigrate_bullet` | Reverse a migration, reopening the source bullet |
| `backlog_bullet` | Move a bullet to the backlog |
| `migration_history` | Trace a bullet's migration chain across days |
| **Notes** | |
| `append_note` | Append a note line to a bullet |
| `update_notes` | Replace all notes on a bullet |
| `clear_notes` | Clear all notes from a bullet |
| **Reorder** | |
| `move_bullet` | Move a bullet to a new position (top, bottom, or index) |

#### Testing with MCP Inspector

Use the [MCP Inspector](https://modelcontextprotocol.io/docs/tools/inspector) to interactively test and debug the server:

```bash
# From source
npx -y @modelcontextprotocol/inspector cargo run -p bulletd-tui -- serve

# Or using the installed binary
npx -y @modelcontextprotocol/inspector bulletd serve
```

This opens a web UI (usually `http://localhost:6274`) where you can browse available tools, invoke them with custom inputs, and inspect results.

## Philosophy

The Bullet Journal method works because of friction. Migrating a task forces you to decide if it's still worth doing. Tasks that aren't worth rewriting die naturally. bulletd preserves that friction digitally — there's no "snooze all" button, no auto-migration. Every open bullet asks you to make a choice.

## Companion: wdttg

bulletd pairs well with [wdttg](https://github.com/plattnum/wdttg-tui) (Where Did The Time Go?) — a terminal time tracker built on the same principles: local-first, plain Markdown, MCP-enabled. Together they cover the daily workflow: bulletd captures *what* you need to do, wdttg captures *how long* it took. Both run as MCP servers, so your AI agent can manage your task log and time entries in the same conversation.

The reality of contract work is that every organization has its own tracking system — Jira, Linear, Asana, whatever — and MCP integrations to those services are either nonexistent, not allowed, or painfully slow. These tools let you stay in flow locally, then reconcile back to the client's system on your own schedule.

## License

MIT — Do whatever you want with it.
