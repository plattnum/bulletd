# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

For releases prior to `0.3.0`, see the [GitHub Releases page](https://github.com/plattnum/bulletd/releases).

## [0.3.0] - 2026-05-09

### Added

- **Work-day-aware migration.** `m` migrates a bullet to the next
  *configured* work day instead of the next calendar day. A new
  `work_days` field in the `[migration]` section of
  `~/.config/bulletd/config.toml` defines which weekdays count — e.g.
  `["Mon", "Tue", "Wed", "Thu", "Fri"]`. Defaults to Mon–Fri when
  absent (existing configs continue to work). Irregular schedules like
  `["Mon", "Thu", "Fri"]` are supported. The MCP `migrate_bullet`
  tool also honors the policy when `target_date` is omitted.
- **`M` migrate picker.** Press `M` (shift-m) on a bullet to open a
  modal listing the next 30 upcoming work days. ↑/↓ select, Enter
  confirms, Esc cancels.
- **Migration target shown inline.** Migrated bullets in the daily
  log render their target next to the text, e.g.
  `Fix flaky test → Tue 2026-05-12`. The edit modal title also shows
  the bullet's id and migration target.
- **`f` follows a migration forward.** On a migrated bullet, press
  `f` to jump to the day it was migrated to.
- **`b` jumps back to source day.** On a bullet that was migrated
  *from* another day, press `b` to jump back to the source day.

### Changed

- **`b` repurposed.** Previously `b` moved the selected bullet to the
  backlog. The backlog action has been removed from the TUI
  keybindings, status bars, help screen, and review-mode prompt. The
  underlying API (`Store::backlog_task`) and the MCP `backlog_bullet`
  tool are unchanged, so existing backlog files continue to parse and
  AI-driven backlog operations still work.

[0.3.0]: https://github.com/plattnum/bulletd/compare/v0.2.0...v0.3.0
