use std::path::PathBuf;

use thiserror::Error;

/// Top-level error type for bulletd-core operations.
#[derive(Debug, Error)]
pub enum Error {
    // -- I/O errors --
    #[error("failed to read file {path}: {source}")]
    ReadFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to write file {path}: {source}")]
    WriteFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to rename {from} to {to}: {source}")]
    AtomicRename {
        from: PathBuf,
        to: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to create directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },

    // -- Parse errors --
    #[error("malformed table row at line {line}: {detail}")]
    MalformedRow { line: usize, detail: String },

    #[error("unknown status emoji: {emoji}")]
    UnknownStatusEmoji { emoji: String },

    #[error("invalid ID format: {id} (expected 8-char lowercase hex)")]
    InvalidIdFormat { id: String },

    #[error("missing columns at line {line}: expected {expected}, found {found}")]
    MissingColumns {
        line: usize,
        expected: usize,
        found: usize,
    },

    #[error("missing date heading in file {path}")]
    MissingDateHeading { path: PathBuf },

    #[error("invalid date in heading: {value}")]
    InvalidDate { value: String },

    #[error("invalid migration link format: {value}")]
    InvalidMigrationLink { value: String },

    // -- Validation errors --
    #[error("bullet not found: {location}/{id}")]
    BulletNotFound { location: String, id: String },

    #[error("invalid status transition: cannot change {from} to {to}")]
    InvalidStatusTransition { from: String, to: String },

    #[error("duplicate ID {id} in file {path}")]
    DuplicateId { id: String, path: PathBuf },

    #[error("bullet {date}/{id} is not a task (type: {bullet_type})")]
    NotATask {
        date: String,
        id: String,
        bullet_type: String,
    },

    #[error("cannot modify {bullet_type} bullet {location}/{id}: events and notes are immutable")]
    ImmutableBullet {
        location: String,
        id: String,
        bullet_type: String,
    },

    // -- Migration errors --
    #[error(
        "cannot unmigrate {date}/{id}: target bullet has been migrated onward — cancel the leaf task first"
    )]
    UnmigrateBlockedByChain { date: String, id: String },

    // -- Config errors --
    #[error("failed to parse config file {path}")]
    ConfigParse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("config file not found at {path} — run `bulletd init` to create one")]
    ConfigNotFound { path: PathBuf },
}

pub type Result<T> = std::result::Result<T, Error>;

/// Outcome of an unmigrate operation.
///
/// Unmigrate always succeeds (or returns an `Error`), but the target bullet
/// may be handled differently depending on whether it was modified.
#[derive(Debug, PartialEq, Eq)]
pub enum UnmigrateOutcome {
    /// Target bullet was untouched and has been deleted.
    TargetDeleted,
    /// Target bullet had been modified — it was cancelled instead of deleted
    /// to preserve any work done there.
    TargetCancelled,
}
