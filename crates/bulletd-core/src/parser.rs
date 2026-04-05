use chrono::NaiveDate;

use crate::error::Error;
use crate::id::validate_id;
use crate::model::{BacklogLog, Bullet, BulletStatus, DailyLog, MigrationRef, MigrationTarget};

/// Result of parsing a bulletd markdown file.
/// The heading can be either a date (daily log) or "Backlog".
#[derive(Debug)]
pub enum ParsedFile {
    DailyLog(DailyLog),
    Backlog(BacklogLog),
}

/// Parse a bulletd markdown file from its string content.
///
/// The file format is:
/// 1. HTML comment header (skipped)
/// 2. H1 heading with date (`# YYYY-MM-DD`) or `# Backlog`
/// 3. GFM table with 5 columns: Status | Bullet | Notes | Migration | ID
pub fn parse_file(content: &str, file_path: &std::path::Path) -> crate::error::Result<ParsedFile> {
    let mut lines = content.lines().enumerate().peekable();

    // Skip HTML comment block
    skip_html_comment(&mut lines, file_path)?;

    // Skip blank lines
    skip_blank_lines(&mut lines);

    // Parse the H1 heading
    let heading = parse_heading(&mut lines, file_path)?;

    // Skip blank lines
    skip_blank_lines(&mut lines);

    // Parse the table (if present)
    let bullets = parse_table(&mut lines, file_path)?;

    match heading {
        Heading::Date(date) => Ok(ParsedFile::DailyLog(DailyLog { date, bullets })),
        Heading::Backlog => Ok(ParsedFile::Backlog(BacklogLog { bullets })),
    }
}

/// Parse a daily log file, returning an error if it's a backlog file.
pub fn parse_daily_log(
    content: &str,
    file_path: &std::path::Path,
) -> crate::error::Result<DailyLog> {
    match parse_file(content, file_path)? {
        ParsedFile::DailyLog(log) => Ok(log),
        ParsedFile::Backlog(_) => Err(Error::MalformedRow {
            line: 0,
            detail: format!(
                "expected a daily log file but {} has a Backlog heading",
                file_path.display()
            ),
        }),
    }
}

/// Parse a backlog file, returning an error if it's a daily log file.
pub fn parse_backlog(
    content: &str,
    file_path: &std::path::Path,
) -> crate::error::Result<BacklogLog> {
    match parse_file(content, file_path)? {
        ParsedFile::Backlog(backlog) => Ok(backlog),
        ParsedFile::DailyLog(log) => Err(Error::MalformedRow {
            line: 0,
            detail: format!(
                "expected a backlog file but {} has a date heading ({})",
                file_path.display(),
                log.date
            ),
        }),
    }
}

// -- Internal types and helpers --

enum Heading {
    Date(NaiveDate),
    Backlog,
}

type LineIter<'a> = std::iter::Peekable<std::iter::Enumerate<std::str::Lines<'a>>>;

fn skip_html_comment(
    lines: &mut LineIter,
    file_path: &std::path::Path,
) -> crate::error::Result<()> {
    // Look for <!-- to start the comment block
    if let Some(&(_, line)) = lines.peek()
        && line.trim_start().starts_with("<!--")
    {
        // Check if comment opens and closes on the same line
        if line.contains("-->") {
            lines.next();
            return Ok(());
        }
        // Consume lines until we find -->
        lines.next();
        for (_, line) in lines.by_ref() {
            if line.contains("-->") {
                return Ok(());
            }
        }
        // Comment was never closed
        return Err(Error::MalformedRow {
            line: 1,
            detail: format!(
                "unclosed HTML comment in {}: found '<!--' but no closing '-->'",
                file_path.display()
            ),
        });
    }
    Ok(())
}

fn skip_blank_lines(lines: &mut LineIter) {
    while let Some(&(_, line)) = lines.peek() {
        if line.trim().is_empty() {
            lines.next();
        } else {
            break;
        }
    }
}

fn parse_heading(
    lines: &mut LineIter,
    file_path: &std::path::Path,
) -> crate::error::Result<Heading> {
    let (_, line) = lines.next().ok_or_else(|| Error::MissingDateHeading {
        path: file_path.to_path_buf(),
    })?;

    let heading_text = line
        .strip_prefix("# ")
        .ok_or_else(|| Error::MissingDateHeading {
            path: file_path.to_path_buf(),
        })?
        .trim();

    if heading_text.eq_ignore_ascii_case("backlog") {
        return Ok(Heading::Backlog);
    }

    let date =
        NaiveDate::parse_from_str(heading_text, "%Y-%m-%d").map_err(|_| Error::InvalidDate {
            value: heading_text.to_string(),
        })?;

    Ok(Heading::Date(date))
}

fn parse_table(
    lines: &mut LineIter,
    _file_path: &std::path::Path,
) -> crate::error::Result<Vec<Bullet>> {
    let mut bullets = Vec::new();

    // Expect table header row: | Status | Bullet | Notes | Migration | ID |
    let header = match lines.next() {
        Some((_, line)) if line.trim_start().starts_with('|') => line,
        _ => return Ok(bullets), // No table present — empty day
    };

    // Validate header has 5 columns
    let header_cols = split_row(header);
    if header_cols.len() != 5 {
        // Not our table format, treat as no table
        return Ok(bullets);
    }

    // Skip separator row: |--------|--------|-------|-----------|-----|
    // Must start with | and contain --- to be a valid GFM separator
    if let Some(&(_, line)) = lines.peek()
        && line.trim_start().starts_with('|')
        && line.contains("---")
    {
        lines.next();
    }

    // Parse data rows
    for (line_num, line) in lines.by_ref() {
        let trimmed = line.trim();
        if trimmed.is_empty() || !trimmed.starts_with('|') {
            break;
        }

        let cols = split_row(trimmed);
        if cols.len() != 5 {
            return Err(Error::MissingColumns {
                line: line_num + 1,
                expected: 5,
                found: cols.len(),
            });
        }

        let bullet = parse_row(&cols, line_num + 1)?;
        bullets.push(bullet);
    }

    Ok(bullets)
}

fn split_row(row: &str) -> Vec<&str> {
    let trimmed = row.trim();
    // Strip leading and trailing pipes, chaining fallbacks correctly
    let after_prefix = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let inner = after_prefix.strip_suffix('|').unwrap_or(after_prefix);
    inner.split('|').map(|s| s.trim()).collect()
}

fn parse_row(cols: &[&str], line_num: usize) -> crate::error::Result<Bullet> {
    let status_str = cols[0];
    let text = cols[1];
    let notes_str = cols[2];
    let migration_str = cols[3];
    let id_str = cols[4];

    // Parse status emoji
    let status = BulletStatus::from_emoji(status_str)?;

    // Parse notes: split on <br> variants
    let notes = parse_notes(notes_str);

    // Parse migration link
    let migration = parse_migration(migration_str, line_num)?;

    // Validate and extract ID
    let id = id_str.trim().to_string();
    if id.is_empty() {
        return Err(Error::MalformedRow {
            line: line_num,
            detail: "empty ID column".to_string(),
        });
    }
    validate_id(&id).map_err(|_| Error::MalformedRow {
        line: line_num,
        detail: format!("invalid ID: {id}"),
    })?;

    Ok(Bullet {
        id,
        status,
        text: text.to_string(),
        notes,
        migration,
    })
}

fn parse_notes(notes_str: &str) -> Vec<String> {
    let trimmed = notes_str.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    // Split on <br> variants case-insensitively, including self-closing forms
    split_on_br(trimmed)
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Split a string on `<br>`, `<br/>`, `<br />` and any case variation.
fn split_on_br(s: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let lower = s.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut start = 0;

    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            // Check for <br>, <br/>, <br />
            let remaining = &lower[i..];
            let tag_len = if remaining.starts_with("<br>") {
                4
            } else if remaining.starts_with("<br/>") {
                5
            } else if remaining.starts_with("<br />") {
                6
            } else {
                i += 1;
                continue;
            };
            // Found a <br> variant — slice from the original string
            result.push(&s[start..i]);
            start = i + tag_len;
            i = start;
        } else {
            i += 1;
        }
    }
    result.push(&s[start..]);
    result
}

/// Parse a migration link from the Migration column.
///
/// Expected formats:
/// - `[to 2026-04-06/d8f2a1b5](./2026-04-06.md)`
/// - `[to backlog/a3c7e9d1](./backlog.md)`
/// - `[from 2026-04-05/c5a1d9e7](./2026-04-05.md)`
fn parse_migration(
    migration_str: &str,
    line_num: usize,
) -> crate::error::Result<Option<MigrationRef>> {
    let trimmed = migration_str.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    // Extract the link text from [text](url) format
    // Use rfind to find the last ]( in case the text itself contains ](
    let without_bracket = trimmed
        .strip_prefix('[')
        .ok_or_else(|| Error::InvalidMigrationLink {
            value: trimmed.to_string(),
        })?;
    let close_pos = without_bracket
        .find("](")
        .ok_or_else(|| Error::InvalidMigrationLink {
            value: trimmed.to_string(),
        })?;
    let link_text = &without_bracket[..close_pos];

    // Parse direction and reference: "to DATE/ID" or "from DATE/ID"
    if let Some(rest) = link_text.strip_prefix("to ") {
        parse_migration_to(rest, line_num)
    } else if let Some(rest) = link_text.strip_prefix("from ") {
        parse_migration_from(rest, line_num)
    } else {
        Err(Error::InvalidMigrationLink {
            value: trimmed.to_string(),
        })
    }
}

fn parse_migration_to(
    reference: &str,
    line_num: usize,
) -> crate::error::Result<Option<MigrationRef>> {
    let (target_str, id) = split_migration_ref(reference, line_num)?;

    let target_date = if target_str == "backlog" {
        MigrationTarget::Backlog
    } else {
        let date =
            NaiveDate::parse_from_str(target_str, "%Y-%m-%d").map_err(|_| Error::InvalidDate {
                value: target_str.to_string(),
            })?;
        MigrationTarget::Date(date)
    };

    Ok(Some(MigrationRef::To {
        target_date,
        target_id: id.to_string(),
    }))
}

fn parse_migration_from(
    reference: &str,
    line_num: usize,
) -> crate::error::Result<Option<MigrationRef>> {
    let (date_str, id) = split_migration_ref(reference, line_num)?;

    let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").map_err(|_| Error::InvalidDate {
        value: date_str.to_string(),
    })?;

    Ok(Some(MigrationRef::From {
        source_date: date,
        source_id: id.to_string(),
    }))
}

fn split_migration_ref(reference: &str, line_num: usize) -> crate::error::Result<(&str, &str)> {
    reference
        .rsplit_once('/')
        .ok_or_else(|| Error::MalformedRow {
            line: line_num,
            detail: format!("migration reference missing '/': {reference}"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn sample_daily_log() -> &'static str {
        include_str!("../../../samples/2026-04-05.md")
    }

    fn sample_daily_log_06() -> &'static str {
        include_str!("../../../samples/2026-04-06.md")
    }

    fn sample_backlog() -> &'static str {
        include_str!("../../../samples/backlog.md")
    }

    #[test]
    fn parse_sample_daily_log_april_05() {
        let log = parse_daily_log(sample_daily_log(), Path::new("2026-04-05.md")).unwrap();

        assert_eq!(log.date, NaiveDate::from_ymd_opt(2026, 4, 5).unwrap());
        assert_eq!(log.bullets.len(), 10);

        // First bullet: done task
        assert_eq!(log.bullets[0].status, BulletStatus::Done);
        assert_eq!(log.bullets[0].text, "Fix Android crash on startup");
        assert_eq!(log.bullets[0].id, "a7f3b2c1");
        assert!(log.bullets[0].notes.is_empty());
        assert!(log.bullets[0].migration.is_none());

        // Second bullet: done task with notes
        assert_eq!(log.bullets[1].status, BulletStatus::Done);
        assert_eq!(log.bullets[1].text, "Review PR #142");
        assert_eq!(log.bullets[1].notes.len(), 2);
        assert_eq!(log.bullets[1].notes[0], "Waiting on CI to pass");
        assert_eq!(log.bullets[1].notes[1], "Approved after second round");

        // Third bullet: event
        assert_eq!(log.bullets[2].status, BulletStatus::Event);
        assert_eq!(log.bullets[2].text, "Sprint planning 10am");

        // Fifth bullet: migrated task with migration link
        assert_eq!(log.bullets[4].status, BulletStatus::Migrated);
        assert_eq!(log.bullets[4].text, "Investigate memory leak");
        assert_eq!(log.bullets[4].notes.len(), 2);
        match &log.bullets[4].migration {
            Some(MigrationRef::To {
                target_date,
                target_id,
            }) => {
                assert_eq!(
                    *target_date,
                    MigrationTarget::Date(NaiveDate::from_ymd_opt(2026, 4, 6).unwrap())
                );
                assert_eq!(target_id, "d8f2a1b5");
            }
            other => panic!("expected MigrationRef::To, got {other:?}"),
        }

        // Sixth bullet: cancelled task
        assert_eq!(log.bullets[5].status, BulletStatus::Cancelled);

        // Seventh bullet: backlogged task with migration to backlog
        assert_eq!(log.bullets[6].status, BulletStatus::Backlogged);
        match &log.bullets[6].migration {
            Some(MigrationRef::To {
                target_date,
                target_id,
            }) => {
                assert_eq!(*target_date, MigrationTarget::Backlog);
                assert_eq!(target_id, "a3c7e9d1");
            }
            other => panic!("expected MigrationRef::To(Backlog), got {other:?}"),
        }

        // Eighth bullet: note
        assert_eq!(log.bullets[7].status, BulletStatus::Note);
        assert_eq!(log.bullets[7].notes.len(), 2);

        // Tenth bullet: note with no notes
        assert_eq!(log.bullets[9].status, BulletStatus::Note);
        assert!(log.bullets[9].notes.is_empty());
    }

    #[test]
    fn parse_sample_daily_log_april_06() {
        let log = parse_daily_log(sample_daily_log_06(), Path::new("2026-04-06.md")).unwrap();

        assert_eq!(log.date, NaiveDate::from_ymd_opt(2026, 4, 6).unwrap());
        assert_eq!(log.bullets.len(), 7);

        // First bullet: open task migrated from another day
        assert_eq!(log.bullets[0].status, BulletStatus::Open);
        match &log.bullets[0].migration {
            Some(MigrationRef::From {
                source_date,
                source_id,
            }) => {
                assert_eq!(*source_date, NaiveDate::from_ymd_opt(2026, 4, 5).unwrap());
                assert_eq!(source_id, "c5a1d9e7");
            }
            other => panic!("expected MigrationRef::From, got {other:?}"),
        }
    }

    #[test]
    fn parse_sample_backlog() {
        let backlog = parse_backlog(sample_backlog(), Path::new("backlog.md")).unwrap();

        assert_eq!(backlog.bullets.len(), 1);
        assert_eq!(backlog.bullets[0].status, BulletStatus::Open);
        assert_eq!(backlog.bullets[0].text, "Update API rate limiting config");
        match &backlog.bullets[0].migration {
            Some(MigrationRef::From {
                source_date,
                source_id,
            }) => {
                assert_eq!(*source_date, NaiveDate::from_ymd_opt(2026, 4, 5).unwrap());
                assert_eq!(source_id, "a3c7e9d1");
            }
            other => panic!("expected MigrationRef::From, got {other:?}"),
        }
    }

    #[test]
    fn parse_empty_table() {
        let content = r#"<!--
  bulletd managed file
-->

# 2026-04-07

| Status | Bullet | Notes | Migration | ID |
|--------|--------|-------|-----------|-----|
"#;
        let log = parse_daily_log(content, Path::new("2026-04-07.md")).unwrap();
        assert_eq!(log.date, NaiveDate::from_ymd_opt(2026, 4, 7).unwrap());
        assert!(log.bullets.is_empty());
    }

    #[test]
    fn parse_new_day_no_table() {
        let content = r#"<!--
  bulletd managed file
-->

# 2026-04-08
"#;
        let log = parse_daily_log(content, Path::new("2026-04-08.md")).unwrap();
        assert_eq!(log.date, NaiveDate::from_ymd_opt(2026, 4, 8).unwrap());
        assert!(log.bullets.is_empty());
    }

    #[test]
    fn parse_wrong_column_count() {
        let content = r#"<!---->

# 2026-04-07

| Status | Bullet | ID |
|--------|--------|----|
| 📌 | Some task | abc12345 |
"#;
        // 3 columns instead of 5 — should not parse as our table
        let log = parse_daily_log(content, Path::new("2026-04-07.md")).unwrap();
        assert!(log.bullets.is_empty());
    }

    #[test]
    fn parse_invalid_emoji() {
        let content = r#"<!---->

# 2026-04-07

| Status | Bullet | Notes | Migration | ID |
|--------|--------|-------|-----------|-----|
| 🦀 | Some task | | | abc12345 |
"#;
        let result = parse_daily_log(content, Path::new("2026-04-07.md"));
        assert!(matches!(result, Err(Error::UnknownStatusEmoji { .. })));
    }

    #[test]
    fn parse_invalid_id() {
        let content = r#"<!---->

# 2026-04-07

| Status | Bullet | Notes | Migration | ID |
|--------|--------|-------|-----------|-----|
| 📌 | Some task | | | BADID |
"#;
        let result = parse_daily_log(content, Path::new("2026-04-07.md"));
        assert!(
            matches!(result, Err(Error::MalformedRow { ref detail, .. }) if detail.contains("invalid ID"))
        );
    }

    #[test]
    fn parse_empty_id_column() {
        let content = r#"<!-- -->

# 2026-04-07

| Status | Bullet | Notes | Migration | ID |
|--------|--------|-------|-----------|-----|
| 📌 | Some task | | |  |
"#;
        let result = parse_daily_log(content, Path::new("2026-04-07.md"));
        assert!(
            matches!(result, Err(Error::MalformedRow { ref detail, .. }) if detail.contains("empty ID"))
        );
    }

    #[test]
    fn parse_notes_with_br_tags() {
        let notes = parse_notes("First line<br>Second line<br>Third line");
        assert_eq!(notes, vec!["First line", "Second line", "Third line"]);
    }

    #[test]
    fn parse_notes_empty() {
        let notes = parse_notes("  ");
        assert!(notes.is_empty());
    }

    #[test]
    fn parse_migration_to_date() {
        let result = parse_migration("[to 2026-04-06/d8f2a1b5](./2026-04-06.md)", 1).unwrap();
        match result {
            Some(MigrationRef::To {
                target_date,
                target_id,
            }) => {
                assert_eq!(
                    target_date,
                    MigrationTarget::Date(NaiveDate::from_ymd_opt(2026, 4, 6).unwrap())
                );
                assert_eq!(target_id, "d8f2a1b5");
            }
            other => panic!("expected Some(To), got {other:?}"),
        }
    }

    #[test]
    fn parse_migration_to_backlog() {
        let result = parse_migration("[to backlog/a3c7e9d1](./backlog.md)", 1).unwrap();
        match result {
            Some(MigrationRef::To {
                target_date,
                target_id,
            }) => {
                assert_eq!(target_date, MigrationTarget::Backlog);
                assert_eq!(target_id, "a3c7e9d1");
            }
            other => panic!("expected Some(To(Backlog)), got {other:?}"),
        }
    }

    #[test]
    fn parse_migration_from() {
        let result = parse_migration("[from 2026-04-05/c5a1d9e7](./2026-04-05.md)", 1).unwrap();
        match result {
            Some(MigrationRef::From {
                source_date,
                source_id,
            }) => {
                assert_eq!(source_date, NaiveDate::from_ymd_opt(2026, 4, 5).unwrap());
                assert_eq!(source_id, "c5a1d9e7");
            }
            other => panic!("expected Some(From), got {other:?}"),
        }
    }

    #[test]
    fn parse_migration_empty() {
        let result = parse_migration("", 1).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn parse_notes_with_br_variants() {
        // Self-closing <br/>
        let notes = split_on_br("First<br/>Second");
        assert_eq!(notes, vec!["First", "Second"]);

        // Self-closing with space <br />
        let notes = split_on_br("First<br />Second");
        assert_eq!(notes, vec!["First", "Second"]);

        // Mixed case
        let notes = split_on_br("First<Br>Second<BR/>Third");
        assert_eq!(notes, vec!["First", "Second", "Third"]);
    }

    #[test]
    fn parse_unclosed_html_comment() {
        let content = "<!--\n  This comment is never closed\n# 2026-04-07\n";
        let result = parse_daily_log(content, Path::new("test.md"));
        assert!(result.is_err());
    }

    #[test]
    fn parse_single_line_html_comment() {
        let content = "<!-- short -->\n\n# 2026-04-07\n";
        let log = parse_daily_log(content, Path::new("test.md")).unwrap();
        assert_eq!(log.date, NaiveDate::from_ymd_opt(2026, 4, 7).unwrap());
    }

    #[test]
    fn parse_daily_log_rejects_backlog_file() {
        let result = parse_daily_log(sample_backlog(), Path::new("backlog.md"));
        assert!(matches!(result, Err(Error::MalformedRow { .. })));
    }

    #[test]
    fn parse_backlog_rejects_daily_log_file() {
        let result = parse_backlog(sample_daily_log(), Path::new("2026-04-05.md"));
        assert!(matches!(result, Err(Error::MalformedRow { .. })));
    }

    #[test]
    fn parse_sample_april_06_deeper() {
        let log = parse_daily_log(sample_daily_log_06(), Path::new("2026-04-06.md")).unwrap();

        // Second bullet: plain open task with no migration
        assert_eq!(log.bullets[1].status, BulletStatus::Open);
        assert_eq!(log.bullets[1].text, "Finish quarterly OKR draft");
        assert!(log.bullets[1].migration.is_none());

        // Last bullet: migrated task with to-link
        assert_eq!(log.bullets[6].status, BulletStatus::Migrated);
        match &log.bullets[6].migration {
            Some(MigrationRef::To {
                target_date,
                target_id,
            }) => {
                assert_eq!(
                    *target_date,
                    MigrationTarget::Date(NaiveDate::from_ymd_opt(2026, 4, 7).unwrap())
                );
                assert_eq!(target_id, "b4e1c8a3");
            }
            other => panic!("expected MigrationRef::To, got {other:?}"),
        }
    }

    #[test]
    fn split_row_no_trailing_pipe() {
        // GFM allows rows without trailing pipe
        let cols = split_row("| ✅ | Some text | notes | | abc12345");
        assert_eq!(cols.len(), 5);
        assert_eq!(cols[0], "✅");
        assert_eq!(cols[4], "abc12345");
    }
}
