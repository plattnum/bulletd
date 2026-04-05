use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::Path;

use crate::error::Error;
use crate::model::{BacklogLog, Bullet, DailyLog, MigrationRef, MigrationTarget};

const FILE_HEADER: &str = r#"<!--
  ⚠️ bulletd managed file — do not hand edit
  Hand editing may break bulletd TUI or MCP server.

  Format: GFM table with columns: Status | Bullet | Notes | Migration | ID

  Status Emoji Reference:
    📌  Open task — not yet acted on
    ✅  Done — completed
    ➡️  Migrated — moved to another day
    ❌  Cancelled — dropped
    📥  Backlogged — moved to backlog.md
    📅  Event — something that happened or is scheduled
    📝  Note — information, context, thoughts

  Notes: Optional context. Use <br> for multiple lines.

  Migration: Traceability for migrated/backlogged tasks. Rendered as clickable links.
    "to YYYY-MM-DD/ID" links to the target file
    "from YYYY-MM-DD/ID" links to the source file
    "to backlog/ID" links to backlog.md

  ID: 8-char hex, unique within this file.
      Cross-file references use date/id format (e.g. 2026-04-05/c5a1d9e7)
-->"#;

const TABLE_HEADER: &str = "| Status | Bullet | Notes | Migration | ID |";
const TABLE_SEPARATOR: &str = "|--------|--------|-------|-----------|-----|";
// Note: column widths are approximate — GFM only requires 3+ dashes per cell.

/// Serialize a DailyLog to its markdown string representation.
/// Always emits the table header and separator, even for empty bullet lists.
pub fn serialize_daily_log(log: &DailyLog) -> String {
    let mut output = String::new();

    // HTML comment header
    output.push_str(FILE_HEADER);
    output.push_str("\n\n");

    // H1 heading with date
    let _ = writeln!(output, "# {}", log.date);
    output.push('\n');

    // Table
    write_table(&mut output, &log.bullets);

    output
}

/// Serialize a BacklogLog to its markdown string representation.
/// Always emits the table header and separator, even for empty bullet lists.
pub fn serialize_backlog(backlog: &BacklogLog) -> String {
    let mut output = String::new();

    // HTML comment header
    output.push_str(FILE_HEADER);
    output.push_str("\n\n");

    // H1 heading
    output.push_str("# Backlog\n\n");

    // Table
    write_table(&mut output, &backlog.bullets);

    output
}

/// Write a DailyLog to disk atomically (write to .tmp, then rename).
pub fn write_daily_log(log: &DailyLog, path: &Path) -> crate::error::Result<()> {
    let content = serialize_daily_log(log);
    atomic_write(path, &content)
}

/// Write a BacklogLog to disk atomically (write to .tmp, then rename).
pub fn write_backlog(backlog: &BacklogLog, path: &Path) -> crate::error::Result<()> {
    let content = serialize_backlog(backlog);
    atomic_write(path, &content)
}

fn atomic_write(path: &Path, content: &str) -> crate::error::Result<()> {
    // Append .tmp suffix to the full filename (not replace extension)
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("bulletd");
    let tmp_path = path.with_file_name(format!("{file_name}.tmp"));

    // Ensure parent directory exists
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent).map_err(|source| Error::CreateDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    // Write to tmp file
    fs::write(&tmp_path, content).map_err(|source| Error::WriteFile {
        path: tmp_path.clone(),
        source,
    })?;

    // Rename to target — clean up tmp on failure
    fs::rename(&tmp_path, path).map_err(|source| {
        let _ = fs::remove_file(&tmp_path);
        Error::AtomicRename {
            from: tmp_path,
            to: path.to_path_buf(),
            source,
        }
    })?;

    Ok(())
}

fn write_table(output: &mut String, bullets: &[Bullet]) {
    output.push_str(TABLE_HEADER);
    output.push('\n');
    output.push_str(TABLE_SEPARATOR);
    output.push('\n');

    for bullet in bullets {
        write_row(output, bullet);
    }
}

fn write_row(output: &mut String, bullet: &Bullet) {
    let status = bullet.status.as_emoji();
    let text = escape_pipe(&bullet.text);
    let notes = format_notes(&bullet.notes);
    let migration = format_migration(&bullet.migration);
    let id = &bullet.id;

    let _ = writeln!(
        output,
        "| {status} | {text} | {notes} | {migration} | {id} |"
    );
}

/// Escape pipe characters in cell content for GFM tables.
fn escape_pipe(s: &str) -> String {
    s.replace('|', "\\|")
}

fn format_notes(notes: &[String]) -> String {
    if notes.is_empty() {
        return String::new();
    }
    notes
        .iter()
        .map(|n| escape_pipe(n))
        .collect::<Vec<_>>()
        .join("<br>")
}

fn format_migration(migration: &Option<MigrationRef>) -> String {
    match migration {
        None => String::new(),
        Some(MigrationRef::To {
            target_date,
            target_id,
        }) => match target_date {
            MigrationTarget::Date(date) => {
                format!("[to {date}/{target_id}](./{date}.md)")
            }
            MigrationTarget::Backlog => {
                format!("[to backlog/{target_id}](./backlog.md)")
            }
        },
        Some(MigrationRef::From {
            source_date,
            source_id,
        }) => {
            format!("[from {source_date}/{source_id}](./{source_date}.md)")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BulletStatus;
    use crate::parser::{parse_backlog, parse_daily_log};
    use chrono::NaiveDate;

    fn sample_daily_log_content() -> &'static str {
        include_str!("../../../samples/2026-04-05.md")
    }

    fn sample_daily_log_06_content() -> &'static str {
        include_str!("../../../samples/2026-04-06.md")
    }

    fn sample_backlog_content() -> &'static str {
        include_str!("../../../samples/backlog.md")
    }

    #[test]
    fn round_trip_daily_log_april_05() {
        let original =
            parse_daily_log(sample_daily_log_content(), Path::new("2026-04-05.md")).unwrap();
        let serialized = serialize_daily_log(&original);
        let reparsed = parse_daily_log(&serialized, Path::new("2026-04-05.md")).unwrap();

        assert_eq!(original, reparsed);
    }

    #[test]
    fn round_trip_daily_log_april_06() {
        let original =
            parse_daily_log(sample_daily_log_06_content(), Path::new("2026-04-06.md")).unwrap();
        let serialized = serialize_daily_log(&original);
        let reparsed = parse_daily_log(&serialized, Path::new("2026-04-06.md")).unwrap();

        assert_eq!(original, reparsed);
    }

    #[test]
    fn round_trip_backlog() {
        let original = parse_backlog(sample_backlog_content(), Path::new("backlog.md")).unwrap();
        let serialized = serialize_backlog(&original);
        let reparsed = parse_backlog(&serialized, Path::new("backlog.md")).unwrap();

        assert_eq!(original, reparsed);
    }

    #[test]
    fn serialize_empty_daily_log() {
        let log = DailyLog {
            date: NaiveDate::from_ymd_opt(2026, 4, 10).unwrap(),
            bullets: vec![],
        };
        let serialized = serialize_daily_log(&log);

        assert!(serialized.contains("# 2026-04-10"));
        assert!(serialized.contains(TABLE_HEADER));
        assert!(serialized.contains(TABLE_SEPARATOR));
        assert!(serialized.contains("⚠️ bulletd managed file"));

        // Should still round-trip
        let reparsed = parse_daily_log(&serialized, Path::new("2026-04-10.md")).unwrap();
        assert_eq!(log, reparsed);
    }

    #[test]
    fn serialize_backlog_heading() {
        let backlog = BacklogLog { bullets: vec![] };
        let serialized = serialize_backlog(&backlog);
        assert!(serialized.contains("# Backlog"));
    }

    #[test]
    fn format_notes_empty() {
        assert_eq!(format_notes(&[]), "");
    }

    #[test]
    fn format_notes_single() {
        assert_eq!(format_notes(&["One note".to_string()]), "One note");
    }

    #[test]
    fn format_notes_multiple() {
        let notes = vec!["First".to_string(), "Second".to_string()];
        assert_eq!(format_notes(&notes), "First<br>Second");
    }

    #[test]
    fn format_migration_none() {
        assert_eq!(format_migration(&None), "");
    }

    #[test]
    fn format_migration_to_date() {
        let mig = Some(MigrationRef::To {
            target_date: MigrationTarget::Date(NaiveDate::from_ymd_opt(2026, 4, 6).unwrap()),
            target_id: "d8f2a1b5".to_string(),
        });
        assert_eq!(
            format_migration(&mig),
            "[to 2026-04-06/d8f2a1b5](./2026-04-06.md)"
        );
    }

    #[test]
    fn format_migration_to_backlog() {
        let mig = Some(MigrationRef::To {
            target_date: MigrationTarget::Backlog,
            target_id: "a3c7e9d1".to_string(),
        });
        assert_eq!(
            format_migration(&mig),
            "[to backlog/a3c7e9d1](./backlog.md)"
        );
    }

    #[test]
    fn format_migration_from() {
        let mig = Some(MigrationRef::From {
            source_date: NaiveDate::from_ymd_opt(2026, 4, 5).unwrap(),
            source_id: "c5a1d9e7".to_string(),
        });
        assert_eq!(
            format_migration(&mig),
            "[from 2026-04-05/c5a1d9e7](./2026-04-05.md)"
        );
    }

    #[test]
    fn atomic_write_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.md");

        let log = DailyLog {
            date: NaiveDate::from_ymd_opt(2026, 4, 10).unwrap(),
            bullets: vec![Bullet {
                id: "a1b2c3d4".to_string(),
                status: BulletStatus::Open,
                text: "Test bullet".to_string(),
                notes: vec![],
                migration: None,
            }],
        };

        write_daily_log(&log, &path).unwrap();
        assert!(path.exists());

        // No .tmp file should remain (tmp path is test.md.tmp)
        let tmp_path = path.with_file_name("test.md.tmp");
        assert!(!tmp_path.exists());

        // Read back and verify
        let content = fs::read_to_string(&path).unwrap();
        let reparsed = parse_daily_log(&content, &path).unwrap();
        assert_eq!(log, reparsed);
    }

    #[test]
    fn atomic_write_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("logs").join("2026-04-10.md");

        let log = DailyLog {
            date: NaiveDate::from_ymd_opt(2026, 4, 10).unwrap(),
            bullets: vec![],
        };

        write_daily_log(&log, &path).unwrap();
        assert!(path.exists());
        assert!(!path.with_file_name("2026-04-10.md.tmp").exists());
    }

    #[test]
    fn round_trip_bullet_text_with_pipe() {
        let log = DailyLog {
            date: NaiveDate::from_ymd_opt(2026, 4, 10).unwrap(),
            bullets: vec![Bullet {
                id: "a1b2c3d4".to_string(),
                status: BulletStatus::Note,
                text: "Fix Foo | Bar".to_string(),
                notes: vec!["Note with | pipe".to_string()],
                migration: None,
            }],
        };
        let serialized = serialize_daily_log(&log);
        let reparsed = parse_daily_log(&serialized, Path::new("2026-04-10.md")).unwrap();
        assert_eq!(log, reparsed);
    }

    #[test]
    fn escape_pipe_in_cell() {
        assert_eq!(escape_pipe("no pipes here"), "no pipes here");
        assert_eq!(escape_pipe("a | b"), "a \\| b");
        assert_eq!(escape_pipe("a | b | c"), "a \\| b \\| c");
    }
}
