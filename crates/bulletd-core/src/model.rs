use chrono::NaiveDate;

/// The status/type marker for a bullet, represented as a single emoji.
///
/// For tasks, the emoji reflects the current state (open, done, migrated, etc.).
/// For events and notes, the emoji is fixed and never changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulletStatus {
    /// 📌 — Open task, not yet acted on
    Open,
    /// ✅ — Task completed
    Done,
    /// ➡️ — Task moved to another day
    Migrated,
    /// ❌ — Task dropped
    Cancelled,
    /// 📥 — Task moved to backlog
    Backlogged,
    /// 📅 — Event (immutable)
    Event,
    /// 📝 — Note (immutable)
    Note,
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
            Self::Event => "📅",
            Self::Note => "📝",
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
            "📅" => Ok(Self::Event),
            "📝" => Ok(Self::Note),
            _ => Err(crate::error::Error::UnknownStatusEmoji {
                emoji: trimmed.to_string(),
            }),
        }
    }

    /// Whether this status represents a task (as opposed to an event or note).
    pub fn is_task(&self) -> bool {
        matches!(
            self,
            Self::Open | Self::Done | Self::Migrated | Self::Cancelled | Self::Backlogged
        )
    }

    /// Whether this bullet is immutable (events and notes cannot change status).
    pub fn is_immutable(&self) -> bool {
        matches!(self, Self::Event | Self::Note)
    }

    /// Display name for error messages.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Done => "done",
            Self::Migrated => "migrated",
            Self::Cancelled => "cancelled",
            Self::Backlogged => "backlogged",
            Self::Event => "event",
            Self::Note => "note",
        }
    }
}

/// The logical type of a bullet (task, event, or note).
/// This is derived from the status — tasks have stateful statuses,
/// events and notes are fixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulletType {
    Task,
    Event,
    Note,
}

impl BulletType {
    /// Derive the bullet type from a status.
    pub fn from_status(status: BulletStatus) -> Self {
        match status {
            BulletStatus::Event => Self::Event,
            BulletStatus::Note => Self::Note,
            _ => Self::Task,
        }
    }

    /// Display name for error messages and CLI prompts.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Event => "event",
            Self::Note => "note",
        }
    }

    /// The default status for a new bullet of this type.
    pub fn default_status(&self) -> BulletStatus {
        match self {
            Self::Task => BulletStatus::Open,
            Self::Event => BulletStatus::Event,
            Self::Note => BulletStatus::Note,
        }
    }
}

/// A reference to a related bullet in another file, used for migration traceability.
///
/// The `target_id`/`source_id` fields are expected to be valid 8-char lowercase hex IDs.
/// The parser validates these during construction; direct struct construction should
/// use `crate::id::validate_id` to enforce the invariant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationRef {
    /// This bullet was migrated TO the referenced location.
    /// Stored as `[to YYYY-MM-DD/ID](./YYYY-MM-DD.md)` or `[to backlog/ID](./backlog.md)`.
    To {
        target_date: MigrationTarget,
        target_id: String,
    },
    /// This bullet was migrated FROM the referenced location.
    /// Stored as `[from YYYY-MM-DD/ID](./YYYY-MM-DD.md)`.
    From {
        source_date: NaiveDate,
        source_id: String,
    },
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
    /// Migration traceability link, if any.
    pub migration: Option<MigrationRef>,
}

impl Bullet {
    /// Get the logical type of this bullet.
    pub fn bullet_type(&self) -> BulletType {
        BulletType::from_status(self.status)
    }

    /// Whether this bullet is a task.
    pub fn is_task(&self) -> bool {
        self.status.is_task()
    }
}

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
            BulletStatus::Event,
            BulletStatus::Note,
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
            ("  📅  ", BulletStatus::Event),
            (" 📝 ", BulletStatus::Note),
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

    #[test]
    fn status_is_task() {
        assert!(BulletStatus::Open.is_task());
        assert!(BulletStatus::Done.is_task());
        assert!(BulletStatus::Migrated.is_task());
        assert!(BulletStatus::Cancelled.is_task());
        assert!(BulletStatus::Backlogged.is_task());
        assert!(!BulletStatus::Event.is_task());
        assert!(!BulletStatus::Note.is_task());
    }

    #[test]
    fn status_is_immutable() {
        assert!(BulletStatus::Event.is_immutable());
        assert!(BulletStatus::Note.is_immutable());
        assert!(!BulletStatus::Open.is_immutable());
        assert!(!BulletStatus::Done.is_immutable());
    }

    #[test]
    fn bullet_type_from_status() {
        assert_eq!(
            BulletType::from_status(BulletStatus::Open),
            BulletType::Task
        );
        assert_eq!(
            BulletType::from_status(BulletStatus::Done),
            BulletType::Task
        );
        assert_eq!(
            BulletType::from_status(BulletStatus::Event),
            BulletType::Event
        );
        assert_eq!(
            BulletType::from_status(BulletStatus::Note),
            BulletType::Note
        );
    }

    #[test]
    fn bullet_type_default_status() {
        assert_eq!(BulletType::Task.default_status(), BulletStatus::Open);
        assert_eq!(BulletType::Event.default_status(), BulletStatus::Event);
        assert_eq!(BulletType::Note.default_status(), BulletStatus::Note);
    }
}
