use chrono::NaiveDate;

/// The status marker for a bullet, represented as a single emoji.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulletStatus {
    /// 📌 — Open, not yet acted on
    Open,
    /// ✅ — Completed
    Done,
    /// ➡️ — Moved to another day
    Migrated,
    /// ❌ — Dropped
    Cancelled,
    /// 📥 — Moved to backlog
    Backlogged,
}

impl BulletStatus {
    /// Convert a status to its emoji string representation.
    pub fn as_emoji(&self) -> &'static str {
        match self {
            Self::Open => "📌",
            Self::Done => "✅",
            Self::Migrated => "➡️",
            Self::Cancelled => "❌",
            Self::Backlogged => "📥",
        }
    }

    /// Parse a status from its emoji string representation.
    pub fn from_emoji(s: &str) -> crate::error::Result<Self> {
        let trimmed = s.trim();
        match trimmed {
            "📌" => Ok(Self::Open),
            "✅" => Ok(Self::Done),
            "➡️" | "➡" => Ok(Self::Migrated),
            "❌" => Ok(Self::Cancelled),
            "📥" => Ok(Self::Backlogged),
            _ => Err(crate::error::Error::UnknownStatusEmoji {
                emoji: trimmed.to_string(),
            }),
        }
    }

    /// Get the display icon for this status from config.
    pub fn display_icon<'a>(&self, icons: &'a crate::config::IconsConfig) -> &'a str {
        match self {
            Self::Open => &icons.open,
            Self::Done => &icons.done,
            Self::Migrated => &icons.migrated,
            Self::Cancelled => &icons.cancelled,
            Self::Backlogged => &icons.backlogged,
        }
    }

    /// Display name for error messages.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Done => "done",
            Self::Migrated => "migrated",
            Self::Cancelled => "cancelled",
            Self::Backlogged => "backlogged",
        }
    }
}

/// A "migrated to" reference — where this bullet was sent.
/// Stored as `[to YYYY-MM-DD/ID](./YYYY-MM-DD.md)` or `[to backlog/ID](./backlog.md)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationTo {
    pub target_date: MigrationTarget,
    pub target_id: String,
}

/// A "migrated from" reference — where this bullet came from.
/// Stored as `[from YYYY-MM-DD/ID](./YYYY-MM-DD.md)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationFrom {
    pub source_date: NaiveDate,
    pub source_id: String,
}

/// The target of a migration — either a specific date or the backlog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationTarget {
    Date(NaiveDate),
    Backlog,
}

/// A single bullet entry in a daily log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bullet {
    /// 8-char lowercase hex ID, unique within the file.
    pub id: String,
    /// The current status (encodes both type and state).
    pub status: BulletStatus,
    /// The bullet text — a short description.
    pub text: String,
    /// Optional notes providing additional context.
    pub notes: Vec<String>,
    /// Where this bullet was migrated/backlogged to, if any.
    pub migrated_to: Option<MigrationTo>,
    /// Where this bullet was migrated from, if any.
    pub migrated_from: Option<MigrationFrom>,
}

impl Bullet {}

/// A daily log containing all bullets for a single day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyLog {
    /// The date this log covers.
    pub date: NaiveDate,
    /// All bullets in creation order.
    pub bullets: Vec<Bullet>,
}

/// A backlog file — same structure as a daily log but without a date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacklogLog {
    pub bullets: Vec<Bullet>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_emoji_round_trip() {
        let statuses = [
            BulletStatus::Open,
            BulletStatus::Done,
            BulletStatus::Migrated,
            BulletStatus::Cancelled,
            BulletStatus::Backlogged,
        ];

        for status in &statuses {
            let emoji = status.as_emoji();
            let parsed = BulletStatus::from_emoji(emoji).unwrap();
            assert_eq!(*status, parsed, "round-trip failed for {emoji}");
        }
    }

    #[test]
    fn status_from_emoji_with_whitespace_all_variants() {
        let cases = [
            (" 📌 ", BulletStatus::Open),
            ("  ✅  ", BulletStatus::Done),
            (" ➡️ ", BulletStatus::Migrated),
            ("  ❌  ", BulletStatus::Cancelled),
            (" 📥 ", BulletStatus::Backlogged),
        ];

        for (input, expected) in &cases {
            let parsed = BulletStatus::from_emoji(input).unwrap();
            assert_eq!(
                *expected, parsed,
                "whitespace parsing failed for input: {input:?}"
            );
        }
    }

    #[test]
    fn status_from_emoji_unknown() {
        let result = BulletStatus::from_emoji("🦀");
        assert!(result.is_err());
    }

    #[test]
    fn migrated_emoji_all_variants_with_whitespace() {
        // ➡️ is U+27A1 + U+FE0F (variation selector)
        // ➡ is U+27A1 alone (no variation selector)
        // Both should parse, with and without surrounding whitespace
        let cases = ["➡️", "➡", " ➡️ ", "  ➡  ", "  ➡️  "];

        for input in &cases {
            let parsed = BulletStatus::from_emoji(input).unwrap();
            assert_eq!(
                BulletStatus::Migrated,
                parsed,
                "migrated parsing failed for input: {input:?}"
            );
        }
    }
}
